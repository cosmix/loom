//! Adjudication subsystem.
//!
//! Disputes filed by agents land in `.work/disputes/<stage>/<n>/request.md`.
//! The orchestrator polls these files every tick:
//!
//! * [`AdjudicatorRegistry::check_pending_disputes`] spawns a worker
//!   thread for each `request.md` that has no `verdict.md` yet (subject
//!   to per-dispute retry caps and the absence of a fresh `.inflight`
//!   marker).
//! * [`AdjudicatorRegistry::apply_pending_verdicts`] scans for verdict
//!   files that haven't been applied (no `applied.marker`) and mutates
//!   stage state accordingly.
//! * [`AdjudicatorRegistry::drain_completed_workers`] drains worker
//!   completion messages off the mpsc channel and joins their handles
//!   so dropped sessions don't leak threads.
//!
//! The registry is owned by the [`Orchestrator`] and lives for the
//! entire daemon run. When `ANTHROPIC_API_KEY` is unset the registry
//! goes into "disabled" mode: workers are never spawned and any
//! pending disputes route directly to `NeedsHumanReview`.

pub mod client;
pub mod feedback;
pub mod prompt;
pub mod verdict;
pub mod worker;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::models::dispute::{
    applied_marker, dispute_dir, request_file, verdict_file, DisputeRequest, DisputeVerdict,
    DisputeVerdictRecord,
};
use crate::models::stage::{Stage, StageStatus};
use crate::plan::amendment::{apply_amendment, AmendmentField, AmendmentPatch, AmendmentRequest};
use crate::verify::transitions::{load_stage, save_stage};

use worker::{
    is_inflight_fresh, spawn_worker, WorkerCompletion, WorkerJob, WorkerOutcome, MAX_WORKER_RETRIES,
};

/// Maximum evidence-loop rounds. After this, the stage escalates to
/// `NeedsHumanReview` instead of looping forever.
pub const MAX_EVIDENCE_ROUNDS: u32 = 3;

/// Adjudicator state owned by the orchestrator.
///
/// Tracks live worker threads keyed by `(stage_id, dispute_id)` and a
/// cooperative-shutdown flag passed to every worker. When the daemon is
/// shutting down, it sets `cancel` and drains `completion_rx` until all
/// handles are joined or a deadline elapses.
pub struct AdjudicatorRegistry {
    /// API key. `None` permanently disables the adjudicator for this
    /// daemon run.
    pub api_key: Option<String>,
    /// Adjudicator endpoint URL. Overridable for tests.
    pub endpoint: String,
    /// Model identifier sent in the JSON body.
    pub model: String,
    /// Per-worker join handles. Removed when a `WorkerCompletion` is
    /// drained off the channel.
    pub handles: HashMap<(String, u32), JoinHandle<()>>,
    /// Cooperative shutdown flag passed to every worker.
    pub cancel: Arc<AtomicBool>,
    /// Worker→orchestrator completion channel.
    pub completion_tx: Sender<WorkerCompletion>,
    pub completion_rx: Receiver<WorkerCompletion>,
}

impl AdjudicatorRegistry {
    /// Construct a registry. `api_key.is_none()` puts the registry in
    /// disabled mode permanently for the lifetime of this daemon run.
    pub fn new(api_key: Option<String>, work_dir: &Path) -> Self {
        let (tx, rx) = mpsc::channel();
        let model = worker::resolve_model(work_dir);
        Self {
            api_key,
            endpoint: client::ANTHROPIC_MESSAGES_URL.to_string(),
            model,
            handles: HashMap::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            completion_tx: tx,
            completion_rx: rx,
        }
    }

    /// True when the registry has been permanently disabled (no key).
    pub fn is_disabled(&self) -> bool {
        self.api_key.is_none()
    }

    /// Scan `.work/disputes/<stage>/<n>/` for pending disputes and
    /// spawn workers for each.
    ///
    /// Skips disputes that:
    /// - already have a `verdict.md` (worker succeeded),
    /// - have a fresh `.inflight` marker (worker in progress),
    /// - have exceeded `MAX_WORKER_RETRIES`,
    /// - belong to a stage whose entire evidence-loop budget is gone
    ///   (escalated to NeedsHumanReview already).
    pub fn check_pending_disputes(&mut self, work_dir: &Path) -> Result<()> {
        let disputes_root = work_dir.join("disputes");
        if !disputes_root.exists() {
            return Ok(());
        }
        for (stage_id, dispute_id) in scan_pending_requests(&disputes_root)? {
            if self.handles.contains_key(&(stage_id.clone(), dispute_id)) {
                continue;
            }
            let req_path = request_file(&disputes_root, &stage_id, dispute_id);
            let request: DisputeRequest = match read_dispute_request(&req_path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %stage_id,
                        dispute = dispute_id,
                        error = %e,
                        "skipping unparseable dispute request",
                    );
                    continue;
                }
            };
            let verdict_path = verdict_file(&disputes_root, &stage_id, dispute_id);
            if verdict_path.exists() {
                continue;
            }
            let inflight = worker::inflight_marker_path(&disputes_root, &stage_id, dispute_id);
            if is_inflight_fresh(&inflight) {
                continue;
            }

            if self.is_disabled() {
                if let Err(e) = escalate_no_api_key(work_dir, &stage_id) {
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %stage_id,
                        error = %e,
                        "failed to escalate stage without API key",
                    );
                }
                continue;
            }

            let attempt = current_attempt_count(work_dir, &stage_id, dispute_id) + 1;
            if attempt > MAX_WORKER_RETRIES {
                if let Err(e) = escalate_attempt_cap(work_dir, &stage_id, dispute_id) {
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %stage_id,
                        dispute = dispute_id,
                        error = %e,
                        "failed to escalate stage after retry cap",
                    );
                }
                continue;
            }

            let stage = match load_stage(&stage_id, work_dir) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %stage_id,
                        error = %e,
                        "could not load stage; skipping dispute",
                    );
                    continue;
                }
            };
            if stage.status != StageStatus::NeedsAdjudication {
                continue;
            }
            if stage.evidence_rounds >= MAX_EVIDENCE_ROUNDS {
                if let Err(e) = escalate_evidence_cap(work_dir, &stage_id) {
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %stage_id,
                        error = %e,
                        "failed to escalate stage after evidence cap",
                    );
                }
                continue;
            }

            let plan_path = resolve_plan_path(work_dir).unwrap_or_else(|| PathBuf::from("PLAN.md"));
            let api_key = match self.api_key.clone() {
                Some(k) => k,
                None => continue,
            };
            let job = WorkerJob {
                work_dir: work_dir.to_path_buf(),
                plan_path,
                stage,
                dispute: request,
                api_key,
                cancellation: Arc::clone(&self.cancel),
                completion_tx: self.completion_tx.clone(),
                endpoint: self.endpoint.clone(),
                model: self.model.clone(),
                attempt,
            };
            let handle = spawn_worker(job);
            self.handles.insert((stage_id, dispute_id), handle);
        }
        Ok(())
    }

    /// Apply verdict files that haven't been applied yet (no
    /// `applied.marker`). Idempotent under crash recovery: a `.applying`
    /// marker is written before mutating stage state and removed only
    /// after `applied.marker` is in place.
    pub fn apply_pending_verdicts(&mut self, work_dir: &Path) -> Result<()> {
        let disputes_root = work_dir.join("disputes");
        if !disputes_root.exists() {
            return Ok(());
        }
        for (stage_id, dispute_id) in scan_pending_verdicts(&disputes_root)? {
            if let Err(e) = self.apply_verdict(work_dir, &stage_id, dispute_id) {
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    dispute = dispute_id,
                    error = %e,
                    "failed to apply verdict",
                );
            }
        }
        Ok(())
    }

    /// Apply a single verdict to the stage. Public so callers under
    /// test can drive a verdict file end-to-end.
    pub fn apply_verdict(
        &mut self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
    ) -> Result<()> {
        let disputes_root = work_dir.join("disputes");
        let verdict_path = verdict_file(&disputes_root, stage_id, dispute_id);
        let applied = applied_marker(&disputes_root, stage_id, dispute_id);
        if applied.exists() {
            return Ok(());
        }
        let applying = dispute_dir(&disputes_root, stage_id, dispute_id).join(".applying");

        let record = read_verdict_record(&verdict_path)?;
        let mut stage = load_stage(stage_id, work_dir)?;

        // Write the .applying marker BEFORE mutating any state so a
        // crash mid-apply is recoverable (the next tick re-enters here
        // and re-applies the same verdict; the work is idempotent).
        if let Some(parent) = applying.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let _ = std::fs::write(&applying, b"");

        let result = self.apply_verdict_inner(work_dir, &mut stage, &record);
        let final_result: Result<()> = (|| -> Result<()> {
            result?;
            save_stage(&stage, work_dir).context("save amended stage after verdict apply")?;
            if let Some(parent) = applied.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&applied, b"")
                .with_context(|| format!("Failed to write {}", applied.display()))?;
            Ok(())
        })();
        // Always remove the .applying marker — success means the
        // applied.marker now exists, failure means we'll retry on
        // the next tick and re-write the marker.
        let _ = std::fs::remove_file(&applying);
        final_result
    }

    fn apply_verdict_inner(
        &self,
        work_dir: &Path,
        stage: &mut Stage,
        record: &DisputeVerdictRecord,
    ) -> Result<()> {
        match &record.verdict {
            DisputeVerdict::Accept { plan_patch, .. } => {
                let plan_path = resolve_plan_path(work_dir)
                    .ok_or_else(|| anyhow::anyhow!("plan source_path missing"))?;
                let request = build_amendment_request(stage.id.clone(), plan_patch, record.id)?;
                apply_amendment(&plan_path, work_dir, request)
                    .context("apply plan amendment from accept verdict")?;
                // Resync the stage's acceptance/wiring from disk (the
                // amendment also rewrites the stage file).
                if let Ok(reloaded) = load_stage(&stage.id, work_dir) {
                    stage.acceptance = reloaded.acceptance;
                    stage.wiring = reloaded.wiring;
                    stage.amendments_applied = reloaded.amendments_applied + 1;
                }
                // Accept verdict closes the evidence loop: clear feedback
                // and re-queue the stage so the agent can retry.
                let _ = feedback::clear_feedback(work_dir, &stage.id);
                transition_to_queued(stage)?;
            }
            DisputeVerdict::Reject {
                reasoning,
                citations,
            } => {
                feedback::append_rejection(work_dir, &stage.id, reasoning, citations)?;
                transition_to_queued(stage)?;
            }
            DisputeVerdict::NeedsMoreEvidence { questions } => {
                feedback::append_questions(work_dir, &stage.id, questions)?;
                stage.evidence_rounds = stage.evidence_rounds.saturating_add(1);
                if stage.evidence_rounds >= MAX_EVIDENCE_ROUNDS {
                    let reason = format!(
                        "Adjudicator evidence loop exhausted ({} rounds)",
                        stage.evidence_rounds
                    );
                    stage.try_request_human_review(reason).ok();
                } else {
                    transition_to_queued(stage)?;
                }
            }
        }
        Ok(())
    }

    /// Drain `completion_rx` and join any handles whose workers
    /// signalled completion. Called once per orchestrator tick.
    pub fn drain_completed_workers(&mut self, work_dir: &Path) -> Result<()> {
        loop {
            let completion = match self.completion_rx.try_recv() {
                Ok(c) => c,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            };
            let key = (completion.stage_id.clone(), completion.dispute_id);
            if let Some(handle) = self.handles.remove(&key) {
                let _ = handle.join();
            }
            match completion.outcome {
                WorkerOutcome::Wrote => {}
                WorkerOutcome::Escalate(reason) => {
                    if let Ok(mut stage) = load_stage(&completion.stage_id, work_dir) {
                        stage.try_request_human_review(reason).ok();
                        if let Err(e) = save_stage(&stage, work_dir) {
                            tracing::warn!(
                                target: "loom::adjudication",
                                stage = %completion.stage_id,
                                error = %e,
                                "failed to persist escalated stage",
                            );
                        }
                    }
                }
                WorkerOutcome::Error(err) => {
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %completion.stage_id,
                        dispute = completion.dispute_id,
                        error = %err,
                        "adjudicator worker reported error",
                    );
                }
            }
        }
        Ok(())
    }

    /// Cooperative shutdown: signal cancellation, then drain completion
    /// messages until `deadline` or all handles are gone. Surviving
    /// workers exit on their own when reqwest finishes/times out; their
    /// `.inflight` markers will go stale and be cleaned on next start.
    pub fn shutdown(&mut self, deadline: Instant) {
        self.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        while Instant::now() < deadline && !self.handles.is_empty() {
            match self.completion_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(c) => {
                    if let Some(handle) = self.handles.remove(&(c.stage_id, c.dispute_id)) {
                        let _ = handle.join();
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if !self.handles.is_empty() {
            tracing::warn!(
                target: "loom::adjudication",
                orphaned = self.handles.len(),
                "adjudicator workers still in-flight at shutdown deadline; exiting anyway",
            );
        }
    }
}

fn transition_to_queued(stage: &mut Stage) -> Result<()> {
    use StageStatus::*;
    let target = Queued;
    // try_transition refuses NeedsAdjudication → Queued unless it knows
    // about it (the foundations stage added that transition). If the
    // stage is somehow in a different status, fall back to a direct
    // assignment with a warning so we don't refuse to apply a verdict
    // because of unrelated state drift.
    if stage.status.can_transition_to(&target) {
        stage.try_transition(target)?;
    } else {
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage.id,
            status = %stage.status,
            "stage not in NeedsAdjudication; forcing queued transition",
        );
        stage.status = target;
        stage.updated_at = chrono::Utc::now();
    }
    Ok(())
}

fn escalate_no_api_key(work_dir: &Path, stage_id: &str) -> Result<()> {
    let mut stage = load_stage(stage_id, work_dir)?;
    stage
        .try_request_human_review("ANTHROPIC_API_KEY not set; adjudicator disabled".to_string())
        .ok();
    save_stage(&stage, work_dir)?;
    Ok(())
}

fn escalate_evidence_cap(work_dir: &Path, stage_id: &str) -> Result<()> {
    let mut stage = load_stage(stage_id, work_dir)?;
    stage
        .try_request_human_review(format!(
            "Evidence loop exhausted at {} rounds",
            MAX_EVIDENCE_ROUNDS
        ))
        .ok();
    save_stage(&stage, work_dir)?;
    Ok(())
}

fn escalate_attempt_cap(work_dir: &Path, stage_id: &str, dispute_id: u32) -> Result<()> {
    let mut stage = load_stage(stage_id, work_dir)?;
    stage
        .try_request_human_review(format!(
            "Adjudicator worker exhausted retry budget for dispute {dispute_id}"
        ))
        .ok();
    save_stage(&stage, work_dir)?;
    Ok(())
}

/// Discover `(stage_id, dispute_id)` pairs that have `request.md` but
/// no `verdict.md`.
fn scan_pending_requests(disputes_root: &Path) -> Result<Vec<(String, u32)>> {
    let mut pending = Vec::new();
    if !disputes_root.exists() {
        return Ok(pending);
    }
    for stage_entry in std::fs::read_dir(disputes_root)? {
        let stage_entry = stage_entry?;
        let path = stage_entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(stage_id) = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        for inner in std::fs::read_dir(&path)? {
            let inner = inner?;
            let inner_path = inner.path();
            if !inner_path.is_dir() {
                continue;
            }
            let Some(name) = inner_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(dispute_id) = name.parse::<u32>() else {
                continue;
            };
            let req = inner_path.join("request.md");
            let ver = inner_path.join("verdict.md");
            if req.exists() && !ver.exists() {
                pending.push((stage_id.clone(), dispute_id));
            }
        }
    }
    pending.sort();
    Ok(pending)
}

/// Discover `(stage_id, dispute_id)` pairs that have `verdict.md` but
/// no `applied.marker`.
fn scan_pending_verdicts(disputes_root: &Path) -> Result<Vec<(String, u32)>> {
    let mut pending = Vec::new();
    if !disputes_root.exists() {
        return Ok(pending);
    }
    for stage_entry in std::fs::read_dir(disputes_root)? {
        let stage_entry = stage_entry?;
        let path = stage_entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(stage_id) = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        for inner in std::fs::read_dir(&path)? {
            let inner = inner?;
            let inner_path = inner.path();
            if !inner_path.is_dir() {
                continue;
            }
            let Some(name) = inner_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(dispute_id) = name.parse::<u32>() else {
                continue;
            };
            let ver = inner_path.join("verdict.md");
            let applied = inner_path.join("applied.marker");
            if ver.exists() && !applied.exists() {
                pending.push((stage_id.clone(), dispute_id));
            }
        }
    }
    pending.sort();
    Ok(pending)
}

fn read_dispute_request(path: &Path) -> Result<DisputeRequest> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse_yaml_frontmatter::<DisputeRequest>(&content)
        .with_context(|| format!("parse dispute request {}", path.display()))
}

fn read_verdict_record(path: &Path) -> Result<DisputeVerdictRecord> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse_yaml_frontmatter::<DisputeVerdictRecord>(&content)
        .with_context(|| format!("parse verdict record {}", path.display()))
}

/// Pull the YAML frontmatter out of a markdown file and deserialize it.
fn parse_yaml_frontmatter<T: serde::de::DeserializeOwned>(content: &str) -> Result<T> {
    let trimmed = content.trim_start();
    let body = trimmed.strip_prefix("---").unwrap_or(trimmed);
    let body = body.trim_start_matches('\n');
    let end = body
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("missing closing '---'"))?;
    let yaml = &body[..end];
    let parsed: T = serde_yaml::from_str(yaml).context("yaml deserialization")?;
    Ok(parsed)
}

/// Look at the verdict file, if present, to see how many adjudicator
/// passes the orchestrator has already recorded. Used to enforce
/// `MAX_WORKER_RETRIES`.
fn current_attempt_count(work_dir: &Path, stage_id: &str, dispute_id: u32) -> u32 {
    let path = verdict_file(&work_dir.join("disputes"), stage_id, dispute_id);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return 0;
    };
    parse_yaml_frontmatter::<DisputeVerdictRecord>(&content)
        .map(|r| r.adjudicator_attempt_count)
        .unwrap_or(0)
}

fn resolve_plan_path(work_dir: &Path) -> Option<PathBuf> {
    let cfg = crate::fs::work_dir::load_config(work_dir).ok().flatten()?;
    let path = cfg.source_path()?;
    if path.is_absolute() {
        Some(path)
    } else {
        let root = work_dir
            .canonicalize()
            .ok()
            .and_then(|wd| wd.parent().map(|p| p.to_path_buf()))?;
        Some(root.join(path))
    }
}

fn build_amendment_request(
    stage_id: String,
    plan_patch: &crate::models::dispute::PlanPatch,
    dispute_id: u32,
) -> Result<AmendmentRequest> {
    let inner = &plan_patch.inner;
    let field = inner
        .get("field")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("plan_patch missing 'field' string"))?;
    let field = match field {
        "acceptance" => AmendmentField::Acceptance,
        "wiring" => AmendmentField::Wiring,
        other => anyhow::bail!("plan_patch field '{other}' must be acceptance|wiring"),
    };
    let patch_obj = inner
        .get("patch")
        .ok_or_else(|| anyhow::anyhow!("plan_patch missing 'patch' object"))?;
    let patch: AmendmentPatch = serde_json::from_value(patch_obj.clone())
        .context("decode AmendmentPatch from plan_patch")?;
    let reason = inner
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(AmendmentRequest {
        stage_id,
        field,
        patch,
        reason,
        dispute_id: Some(dispute_id.to_string()),
    })
}
