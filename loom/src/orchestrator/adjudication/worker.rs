//! Worker thread that drives one dispute → verdict round-trip.
//!
//! The orchestrator's `check_pending_disputes` (in `mod.rs`) spawns one
//! worker thread per pending dispute. The worker:
//!
//! 1. Creates an `.inflight` marker so the main loop does not re-spawn it.
//! 2. Reads the request, builds the prompt, calls the Anthropic API.
//! 3. Parses and validates the verdict.
//! 4. Persists the verdict (or an escalation marker) to disk.
//! 5. Removes the `.inflight` marker on drop (via `InflightGuard`).
//! 6. Reports completion via a `mpsc::Sender<WorkerCompletion>` so the
//!    orchestrator can `join()` the thread handle and free its slot.
//!
//! Errors (HTTP timeout, panic, etc.) cause the worker to exit without
//! writing `verdict.md`. The `.inflight` marker is still removed
//! (drop) so the next main-loop tick can either retry (if the
//! adjudicator_attempt_count is under the cap) or escalate.

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::models::dispute::{
    applied_marker, dispute_dir, request_file, verdict_file, DisputeRequest, DisputeVerdict,
    DisputeVerdictRecord,
};
use crate::models::stage::Stage;

use super::client::ADJUDICATOR_MODEL;
use super::feedback;
use super::prompt;
use super::verdict::{parse_and_validate, ValidationOutcome};

/// Maximum number of worker spawn attempts for a single dispute before
/// the orchestrator escalates the stage to `NeedsHumanReview`.
pub const MAX_WORKER_RETRIES: u32 = 3;

/// Workers older than this are considered stale (their `.inflight`
/// marker is ignored and the orchestrator re-spawns them). Tracks
/// the timeout reqwest itself enforces — adding a small slack so the
/// race between "client returns timeout" and "marker mtime updates" is
/// observably benign.
pub const INFLIGHT_TIMEOUT_SECS: u64 = 360;

/// Filename used for the per-dispute in-flight marker. Lives in the
/// dispute directory alongside `request.md` / `verdict.md`.
pub const INFLIGHT_FILENAME: &str = ".inflight";

/// Message sent on the orchestrator's mpsc channel when a worker has
/// finished (success, error, or panic-recovery). Carries enough info
/// for the orchestrator to find and remove the matching JoinHandle.
#[derive(Debug, Clone)]
pub struct WorkerCompletion {
    pub stage_id: String,
    pub dispute_id: u32,
    pub outcome: WorkerOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerOutcome {
    /// Verdict file written successfully.
    Wrote,
    /// Verdict was so degenerate that the stage must be escalated to
    /// `NeedsHumanReview`. The orchestrator handles the transition.
    Escalate(String),
    /// HTTP/parse/etc. error — main loop will retry until the worker
    /// attempt cap is reached.
    Error(String),
}

/// RAII guard that removes the `.inflight` marker when dropped.
struct InflightGuard {
    path: PathBuf,
}

impl InflightGuard {
    /// Create the `.inflight` marker. Returns `Err` if another worker
    /// already owns it (the main loop will skip this dispute until the
    /// existing marker expires or is removed).
    fn create(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("Failed to create inflight marker {}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Layout helper for the per-dispute inflight marker.
pub fn inflight_marker_path(disputes_root: &Path, stage_id: &str, id: u32) -> PathBuf {
    dispute_dir(disputes_root, stage_id, id).join(INFLIGHT_FILENAME)
}

/// Return `true` if the inflight marker exists AND is younger than
/// [`INFLIGHT_TIMEOUT_SECS`]. Stale markers are ignored so a crashed
/// worker's slot can be reclaimed.
pub fn is_inflight_fresh(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    match modified.elapsed() {
        Ok(elapsed) => elapsed.as_secs() < INFLIGHT_TIMEOUT_SECS,
        Err(_) => true, // clock skew — be conservative, treat as fresh
    }
}

/// Configuration for a worker thread.
pub struct WorkerJob {
    pub work_dir: PathBuf,
    pub plan_path: PathBuf,
    pub stage: Stage,
    pub dispute: DisputeRequest,
    pub api_key: String,
    pub cancellation: Arc<AtomicBool>,
    pub completion_tx: Sender<WorkerCompletion>,
    /// Adjudicator endpoint URL. Defaults to the live Anthropic API;
    /// tests inject a wiremock URL.
    pub endpoint: String,
    /// Model name used in the JSON body.
    pub model: String,
    /// Attempt counter (1-based) recorded in the verdict file.
    pub attempt: u32,
}

/// Spawn a worker thread. The returned `JoinHandle` should be stored in
/// the orchestrator's registry so cooperative shutdown can join it.
///
/// Uses `thread::spawn` so the HTTP call never blocks the orchestrator's
/// poll loop. Each worker runs entirely independently.
pub fn spawn_worker(job: WorkerJob) -> JoinHandle<()> {
    thread::spawn(move || run(job))
}

fn run(job: WorkerJob) {
    let WorkerJob {
        work_dir,
        plan_path,
        stage,
        dispute,
        api_key,
        cancellation,
        completion_tx,
        endpoint,
        model,
        attempt,
    } = job;
    let stage_id = stage.id.clone();
    let dispute_id = dispute.id;
    let disputes_root = work_dir.join("disputes");
    let inflight = inflight_marker_path(&disputes_root, &stage_id, dispute_id);

    let _guard = match InflightGuard::create(inflight.clone()) {
        Ok(g) => g,
        Err(e) => {
            let _ = completion_tx.send(WorkerCompletion {
                stage_id: stage_id.clone(),
                dispute_id,
                outcome: WorkerOutcome::Error(format!("inflight conflict: {e}")),
            });
            return;
        }
    };

    let outcome = run_inner(
        &work_dir,
        &plan_path,
        &stage,
        &dispute,
        &api_key,
        cancellation,
        &endpoint,
        &model,
        attempt,
    );
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => WorkerOutcome::Error(format!("{e:#}")),
    };
    let _ = completion_tx.send(WorkerCompletion {
        stage_id,
        dispute_id,
        outcome,
    });
}

#[allow(clippy::too_many_arguments)]
fn run_inner(
    work_dir: &Path,
    plan_path: &Path,
    stage: &Stage,
    dispute: &DisputeRequest,
    api_key: &str,
    cancellation: Arc<AtomicBool>,
    endpoint: &str,
    model: &str,
    attempt: u32,
) -> Result<WorkerOutcome> {
    if cancellation.load(Ordering::Relaxed) {
        return Ok(WorkerOutcome::Error("cancelled before HTTP".to_string()));
    }
    let prompt = prompt::build(plan_path, stage, dispute, work_dir)
        .context("build adjudicator prompt")?;
    let raw = super::client::call_anthropic_with(
        api_key,
        &prompt,
        Arc::clone(&cancellation),
        model,
        endpoint,
    )
    .context("Anthropic API call")?;

    let outcome = parse_and_validate(&raw);
    match outcome {
        ValidationOutcome::Verdict(verdict) => {
            persist_verdict(work_dir, stage, dispute, &verdict, model, attempt)
                .context("persist verdict")?;
            propagate_feedback(work_dir, &stage.id, &verdict)
                .context("write feedback file")?;
            Ok(WorkerOutcome::Wrote)
        }
        ValidationOutcome::Escalate { reason } => Ok(WorkerOutcome::Escalate(reason)),
    }
}

fn persist_verdict(
    work_dir: &Path,
    stage: &Stage,
    dispute: &DisputeRequest,
    verdict: &DisputeVerdict,
    model: &str,
    attempt: u32,
) -> Result<()> {
    let disputes_root = work_dir.join("disputes");
    let path = verdict_file(&disputes_root, &stage.id, dispute.id);
    let record = DisputeVerdictRecord {
        id: dispute.id,
        stage_id: stage.id.clone(),
        verdict: verdict.clone(),
        adjudicator_attempt_count: attempt,
        created_at: Utc::now(),
        model: model.to_string(),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let yaml = serde_yaml::to_string(&record).context("serialize verdict record")?;
    let body = format!(
        "---\n{yaml}---\n\n# Verdict for {} dispute {}\n",
        stage.id, dispute.id,
    );
    std::fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn propagate_feedback(work_dir: &Path, stage_id: &str, verdict: &DisputeVerdict) -> Result<()> {
    match verdict {
        DisputeVerdict::Reject {
            reasoning,
            citations,
        } => feedback::append_rejection(work_dir, stage_id, reasoning, citations),
        DisputeVerdict::NeedsMoreEvidence { questions } => {
            feedback::append_questions(work_dir, stage_id, questions)
        }
        // Accept verdicts don't produce agent-facing feedback; the
        // plan amendment itself is the signal.
        DisputeVerdict::Accept { .. } => Ok(()),
    }
}

/// Helper used by `mod.rs::apply_pending_verdicts`: returns the path
/// where the verdict file lives for a given (stage, dispute).
pub fn verdict_path(work_dir: &Path, stage_id: &str, id: u32) -> PathBuf {
    let disputes_root = work_dir.join("disputes");
    verdict_file(&disputes_root, stage_id, id)
}

/// Helper used by `mod.rs::apply_pending_verdicts`: returns the path
/// to the applied-marker for a given (stage, dispute).
pub fn applied_marker_path(work_dir: &Path, stage_id: &str, id: u32) -> PathBuf {
    let disputes_root = work_dir.join("disputes");
    applied_marker(&disputes_root, stage_id, id)
}

/// Helper used by `mod.rs::check_pending_disputes`: returns the path
/// to the request file.
pub fn request_path(work_dir: &Path, stage_id: &str, id: u32) -> PathBuf {
    let disputes_root = work_dir.join("disputes");
    request_file(&disputes_root, stage_id, id)
}

/// Default model string for new workers, honouring
/// `.work/config.toml::[adjudication].model` if present.
pub fn resolve_model(work_dir: &Path) -> String {
    let config_path = work_dir.join("config.toml");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(value) = toml::from_str::<toml::Value>(&content) {
            if let Some(model) = value
                .get("adjudication")
                .and_then(|a| a.get("model"))
                .and_then(|m| m.as_str())
            {
                return model.to_string();
            }
        }
    }
    ADJUDICATOR_MODEL.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflight_guard_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".inflight");
        {
            let _g = InflightGuard::create(path.clone()).unwrap();
            assert!(path.exists());
        }
        assert!(!path.exists(), "drop should remove marker");
    }

    #[test]
    fn inflight_guard_refuses_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".inflight");
        std::fs::write(&path, b"x").unwrap();
        assert!(InflightGuard::create(path).is_err());
    }

    #[test]
    fn fresh_inflight_marker_is_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".inflight");
        std::fs::write(&path, b"").unwrap();
        assert!(is_inflight_fresh(&path));
    }

    #[test]
    fn missing_inflight_marker_is_not_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".inflight");
        assert!(!is_inflight_fresh(&path));
    }

    #[test]
    fn resolve_model_falls_back_to_default_when_unset() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(resolve_model(tmp.path()), ADJUDICATOR_MODEL);
    }

    #[test]
    fn resolve_model_reads_config_override() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config.toml"),
            "[adjudication]\nmodel = \"claude-haiku-test\"\n",
        )
        .unwrap();
        assert_eq!(resolve_model(tmp.path()), "claude-haiku-test");
    }
}
