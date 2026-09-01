//! The spawn side of adjudication: what the daemon needs to know to start a
//! session for a dispute, and what the CLI needs to record its verdict.
//!
//! An adjudication session is an ordinary loom session — spawned by the daemon
//! into a terminal, running in the MAIN REPOSITORY like a knowledge session,
//! with the full tool surface. Nothing here runs a model itself; the daemon
//! starts the session and observes `verdict.md` appearing on a later tick, the
//! same way merge resolution observes `loom stage merge --resolved`.
//!
//! All of the state that bounds the loop lives on disk, so a daemon restart
//! mid-adjudication neither loses a live session nor resets its budget:
//!
//! * liveness — the session RECORD under `.loom/work/sessions/`, probed through
//!   [`crate::orchestrator::session_registry::live_sessions_for_stage`], the
//!   same helper the stage executor asks before spawning;
//! * budget — `attempts` in the dispute directory, mirroring the on-disk
//!   merge-resolver attempt counter.

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};

use crate::models::dispute::{
    dispute_dir, verdict_file, DisputeRequest, DisputeVerdict, DisputeVerdictRecord,
};
use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::orchestrator::terminal::backend::SessionBackend;

/// Model adjudication sessions run on when `.loom/work/config.toml` names none.
///
/// A `claude --model` argument (an alias such as `opus`, or a full model id),
/// never an API model identifier.
pub const DEFAULT_ADJUDICATION_MODEL: &str = "opus";

/// Adjudication sessions started for a single dispute before the stage
/// escalates to `NeedsHumanReview` instead of being handed another one.
pub const MAX_ADJUDICATION_ATTEMPTS: u32 = 3;

/// Per-dispute spawn counter, written by the daemon alongside `request.md`.
const ATTEMPTS_FILENAME: &str = "attempts";

/// The file an adjudication session writes its raw JSON verdict to before
/// handing it to `loom stage adjudicate`.
///
/// Deliberately NOT `verdict.md`: that record is written only by the guarded
/// CLI path (see `commands/stage/adjudicate.rs`), which is what keeps the
/// dispute directory's authority split intact. This is the session's draft,
/// and it grants nothing on its own.
const VERDICT_DRAFT_FILENAME: &str = "verdict.json";

/// One dispute the daemon should start an adjudication session for, with
/// everything the signal needs already resolved.
#[derive(Debug, Clone)]
pub struct AdjudicationJob {
    pub stage: Stage,
    pub request: DisputeRequest,
    /// The live plan markdown, so the signal can quote the criterion as the
    /// plan states it.
    pub plan_path: PathBuf,
}

/// An adjudication session the daemon has just started, for the operator-
/// facing line the caller prints.
pub struct StartedAdjudication {
    pub stage_id: String,
    pub dispute_id: u32,
    pub session_id: String,
}

impl super::AdjudicatorRegistry {
    /// Start an adjudication session for every dispute that needs one.
    ///
    /// A spawn failure escalates that dispute's stage rather than aborting the
    /// pass: it means no adjudicator can run here at all, and a dispute left
    /// waiting on a session that will never start hangs silently.
    pub fn start_pending_adjudications(
        &self,
        backend: &SessionBackend,
        work_dir: &Path,
        repo_root: &Path,
    ) -> Result<Vec<StartedAdjudication>> {
        let mut started = Vec::new();
        for job in self.disputes_awaiting_session(work_dir)? {
            match spawn_for(backend, &job, work_dir, repo_root) {
                Ok(session) => started.push(StartedAdjudication {
                    stage_id: job.stage.id,
                    dispute_id: job.request.id,
                    session_id: session.id,
                }),
                Err(error) => {
                    let error = format!("{error:#}");
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %job.stage.id,
                        dispute = job.request.id,
                        %error,
                        "could not spawn an adjudication session; escalating the stage",
                    );
                    super::escalate_adjudicator_unavailable(work_dir, &job.stage.id, &error);
                }
            }
        }
        Ok(started)
    }
}

/// Start one adjudication session: main repo, no worktree, briefed by an
/// adjudication signal.
///
/// Mirrors the merge-resolution spawn, except that the session is NOT
/// registered in the orchestrator's `active_sessions`: the stage's own agent
/// may still hold that slot, and the monitor's crash reporting is for stage
/// execution, not for a judge whose exit is ordinary. The saved session record
/// is what [`live_adjudication_session`] probes.
fn spawn_for(
    backend: &SessionBackend,
    job: &AdjudicationJob,
    work_dir: &Path,
    repo_root: &Path,
) -> Result<Session> {
    let session = Session::new_adjudication(&job.stage.id);
    let signal_path = crate::orchestrator::signals::generate_adjudication_signal(
        &session,
        &job.stage,
        &job.request,
        &job.plan_path,
        work_dir,
    )
    .context("Failed to generate adjudication signal")?;

    let spawned = backend
        .spawn_adjudication_session(&job.stage, session, &signal_path, repo_root)
        .context("Failed to spawn adjudication session")?;

    crate::fs::session_files::save_session(&spawned, work_dir)
        .context("Failed to save the adjudication session record")?;
    Ok(spawned)
}

/// Where the adjudication session writes its JSON verdict.
pub fn verdict_draft_file(work_dir: &Path, stage_id: &str, dispute_id: u32) -> PathBuf {
    dispute_dir(&work_dir.join("disputes"), stage_id, dispute_id).join(VERDICT_DRAFT_FILENAME)
}

fn attempts_file(work_dir: &Path, stage_id: &str, dispute_id: u32) -> PathBuf {
    dispute_dir(&work_dir.join("disputes"), stage_id, dispute_id).join(ATTEMPTS_FILENAME)
}

/// How many adjudication sessions this dispute has already been given.
pub fn attempt_count(work_dir: &Path, stage_id: &str, dispute_id: u32) -> u32 {
    std::fs::read_to_string(attempts_file(work_dir, stage_id, dispute_id))
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

/// Count one more adjudication attempt and return the new total.
///
/// Counted when the daemon decides to hand the dispute a session, not when
/// that session succeeds: a spawn that fails escalates immediately (see
/// `escalate_adjudicator_unavailable`), so the budget only ever has to bound
/// sessions that started and then died without writing a verdict.
pub(super) fn record_attempt(work_dir: &Path, stage_id: &str, dispute_id: u32) -> u32 {
    let path = attempts_file(work_dir, stage_id, dispute_id);
    let next = attempt_count(work_dir, stage_id, dispute_id).saturating_add(1);
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                target: "loom::adjudication",
                stage = %stage_id,
                dispute = dispute_id,
                %error,
                "could not create the dispute directory; adjudication attempts are not being counted",
            );
            return next;
        }
    }
    if let Err(error) = std::fs::write(&path, next.to_string()) {
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage_id,
            dispute = dispute_id,
            %error,
            "could not persist the adjudication attempt count",
        );
    }
    next
}

/// The id of a live adjudication session already working on `stage_id`, if any.
///
/// Answered from the session record plus the backend's own liveness probe, so
/// a session started by a previous daemon still counts and a session whose
/// process is gone does not. A stage in `NeedsAdjudication` has at most one
/// dispute in flight (filing another requires the stage to leave that status),
/// so one live session per STAGE is the right granularity — and two
/// adjudicators in the same main repository is exactly what must not happen.
pub(super) fn live_adjudication_session(work_dir: &Path, stage_id: &str) -> Option<String> {
    let sessions =
        crate::orchestrator::session_registry::live_sessions_for_stage(work_dir, stage_id)
            .unwrap_or_else(|error| {
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    %error,
                    "could not list live sessions; assuming no adjudication session is running",
                );
                Vec::new()
            });
    sessions
        .into_iter()
        .find(|session| session.session_type == SessionType::Adjudication)
        .map(|session| session.id)
}

/// Write `verdict.md` for a dispute.
///
/// The daemon's `apply_pending_verdicts` picks the record up on its next tick;
/// nothing here touches stage state.
pub fn persist_verdict(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    verdict: &DisputeVerdict,
    model: &str,
    attempt: u32,
) -> Result<()> {
    let path = verdict_file(&work_dir.join("disputes"), stage_id, dispute_id);
    let record = DisputeVerdictRecord {
        id: dispute_id,
        stage_id: stage_id.to_string(),
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
    let body = format!("---\n{yaml}---\n\n# Verdict for {stage_id} dispute {dispute_id}\n");
    std::fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Model for adjudication sessions, honouring
/// `.loom/work/config.toml::[adjudication].model` if present.
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
    DEFAULT_ADJUDICATION_MODEL.to_string()
}

/// Read a dispute request straight from disk, for callers outside the daemon
/// (the `loom stage adjudicate` command) that must confirm the dispute exists
/// before a verdict is recorded against it.
pub fn read_request(work_dir: &Path, stage_id: &str, dispute_id: u32) -> Result<DisputeRequest> {
    let path =
        crate::models::dispute::request_file(&work_dir.join("disputes"), stage_id, dispute_id);
    super::scan::read_dispute_request(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_start_at_zero_and_accumulate() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path();
        assert_eq!(attempt_count(work, "s1", 1), 0);
        assert_eq!(record_attempt(work, "s1", 1), 1);
        assert_eq!(record_attempt(work, "s1", 1), 2);
        assert_eq!(attempt_count(work, "s1", 1), 2);
        // Counted per dispute, not per stage.
        assert_eq!(attempt_count(work, "s1", 2), 0);
    }

    #[test]
    fn attempt_count_survives_a_reread() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path();
        record_attempt(work, "s1", 1);
        // A fresh read is what a restarted daemon does.
        assert_eq!(attempt_count(work, "s1", 1), 1);
    }

    #[test]
    fn draft_and_attempt_paths_live_in_the_dispute_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path();
        let dir = dispute_dir(&work.join("disputes"), "s1", 3);
        assert_eq!(verdict_draft_file(work, "s1", 3), dir.join("verdict.json"));
        assert_eq!(attempts_file(work, "s1", 3), dir.join("attempts"));
    }

    #[test]
    fn resolve_model_falls_back_to_default_when_unset() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(resolve_model(tmp.path()), DEFAULT_ADJUDICATION_MODEL);
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

    #[test]
    fn persist_verdict_writes_a_parseable_record() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path();
        persist_verdict(
            work,
            "s1",
            1,
            &DisputeVerdict::NeedsMoreEvidence {
                questions: vec!["why?".to_string()],
            },
            "opus",
            2,
        )
        .unwrap();
        let path = verdict_file(&work.join("disputes"), "s1", 1);
        let content = std::fs::read_to_string(&path).unwrap();
        let record: DisputeVerdictRecord = super::super::scan::parse_yaml_frontmatter(&content)
            .expect("verdict.md must parse back as a record");
        assert_eq!(record.adjudicator_attempt_count, 2);
        assert_eq!(record.model, "opus");
    }

    #[test]
    fn no_live_session_when_none_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(live_adjudication_session(tmp.path(), "s1").is_none());
    }
}
