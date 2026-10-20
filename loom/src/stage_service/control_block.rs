//! Narrow daemon-side stage block transition.
//!
//! `.loom/work/` is read-only from a stage worktree, so an agent that discovers it
//! cannot proceed has no way to write `stages/<id>.md` itself — which left the
//! sanctioned "say why you are stuck" command unusable from the only place it
//! was ever needed. The daemon owns the write for the same reason it owns
//! completion and dispute persistence, and by the time this runs
//! `server/self_service.rs` has already established that the caller owns the
//! stage.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;

use super::Response;
use crate::handoff::load_trusted_session_checkpoint;
use crate::handoff::schema::CompletionPhase;
use crate::verify::transitions::{load_stage, update_stage};

/// A stage agent's `loom stage block`: refused while the stage session's
/// latest signed `loom stage complete` attempt failed with at least one
/// failing `acceptance-<i>` criterion, otherwise [`handle_block_stage`].
///
/// A failing criterion has a route that keeps the stage moving: a dispute,
/// judged by an adjudicator and escalated to the operator once the budget is
/// spent. A block there parks the stage as an intentional block that is never
/// retried, so the work waits on an operator for a question the adjudicator
/// could have answered. A failed completion naming no criterion (missing
/// sandbox, setup error) has no dispute route, so the block stays open.
/// Operator blocks and the daemon's own signing block call
/// [`handle_block_stage`] directly.
pub(crate) fn handle_agent_block_stage(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
) -> Result<Response> {
    crate::validation::validate_id(stage_id).context("invalid block stage id")?;
    let failing = failing_criterion_indices(work_dir, stage_id);
    if !failing.is_empty() {
        return Ok(Response::Error {
            message: failed_completion_refusal(stage_id, &failing),
        });
    }
    handle_block_stage(work_dir, stage_id, reason)
}

/// Indices of the `acceptance-<i>` criteria that failed in the current session's
/// latest signed `ToolFailed` completion attempt. A record that cannot be read
/// yields none: the agent's way to stop must not hinge on it.
fn failing_criterion_indices(work_dir: &Path, stage_id: &str) -> Vec<usize> {
    let session = match load_stage(stage_id, work_dir) {
        Ok(stage) => stage.session,
        Err(error) => {
            tracing::warn!(stage_id, error = %format!("{error:#}"), "block: stage unreadable");
            return Vec::new();
        }
    };
    let Some(session) = session else {
        return Vec::new();
    };
    match load_trusted_session_checkpoint(stage_id, &session, work_dir) {
        Ok(checkpoint) => checkpoint
            .and_then(|checkpoint| checkpoint.latest)
            .filter(|latest| latest.phase == CompletionPhase::ToolFailed)
            .map(|latest| {
                let mut indices: Vec<usize> = latest
                    .verification
                    .criteria
                    .iter()
                    .filter(|result| !result.passed)
                    .filter_map(|result| result.id.strip_prefix("acceptance-")?.parse().ok())
                    .collect();
                indices.sort_unstable();
                indices.dedup();
                indices
            })
            .unwrap_or_default(),
        Err(error) => {
            tracing::warn!(
                stage_id,
                session_id = %session,
                error = %format!("{error:#}"),
                "block: completion record unreadable"
            );
            Vec::new()
        }
    }
}

fn failed_completion_refusal(stage_id: &str, failing: &[usize]) -> String {
    let flags: Vec<String> = failing
        .iter()
        .map(|index| format!("--criterion-index {index}"))
        .collect();
    format!(
        "cannot block stage '{stage_id}': the latest `loom stage complete` shows failing \
         criteria. A failing check is disputed, never blocked: `loom stage dispute-criteria \
         {stage_id} {} --reason \"...\"` names every failing criterion in one filing \
         (`--field wiring` or `--field wiring-tests` for wiring entries, `--field impact-tests \
         --test <id>` for failing impact-selected tests); open review findings go to `loom \
         stage dispute-findings`, test-integrity events to `loom stage dispute-integrity`, a \
         frozen contract to `loom stage dispute-contract`. A filing past the dispute budget \
         goes to the operator automatically.",
        flags.join(" ")
    )
}

/// Apply `Blocked` to `stage_id`, recording `reason` as its close reason.
///
/// A refused transition comes back as [`Response::Error`] rather than an
/// `Err`: it is an answer about the stage, not a failure of the request, and
/// the message names the status it was refused from because an agent reading
/// the reply has no other way to learn why. "Already blocked" and "already
/// completed" call for different next moves.
pub(crate) fn handle_block_stage(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
) -> Result<Response> {
    // The id arrives unvalidated from the wire and is resolved to a path
    // below; traversal shapes die before any file is touched.
    crate::validation::validate_id(stage_id).context("invalid block stage id")?;

    let mut refusal = None;
    update_stage(stage_id, work_dir, |stage| {
        if let Err(error) = stage.try_mark_blocked() {
            refusal = Some(format!("cannot block stage '{stage_id}': {error:#}"));
            return Ok(());
        }
        stage.close_reason = Some(reason.to_string());
        // A block closes the attempt with its own reason: an earlier attempt's
        // crash evidence would re-arm auto-retry and hide this block from
        // attention and from the daemon's retirement of the session.
        stage.failure_info = None;
        stage.updated_at = Utc::now();
        Ok(())
    })
    .with_context(|| format!("failed to persist blocked state for stage '{stage_id}'"))?;

    Ok(match refusal {
        Some(message) => Response::Error { message },
        None => Response::Ok,
    })
}

#[cfg(test)]
#[path = "control_block_agent_tests.rs"]
mod agent_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Stage, StageStatus};
    use crate::verify::transitions::{load_stage, save_stage};
    use tempfile::TempDir;

    fn stage_in(work_dir: &Path, stage_id: &str, status: StageStatus) {
        let mut stage = Stage::new(stage_id.to_string(), None);
        stage.id = stage_id.to_string();
        stage.status = status;
        save_stage(&stage, work_dir).unwrap();
    }

    #[test]
    fn blocking_an_executing_stage_records_the_reason() {
        let temp = TempDir::new().unwrap();
        stage_in(temp.path(), "build-api", StageStatus::Executing);

        let response =
            handle_block_stage(temp.path(), "build-api", "criterion 41 is unrunnable").unwrap();

        assert!(matches!(response, Response::Ok));
        let stage = load_stage("build-api", temp.path()).unwrap();
        assert_eq!(stage.status, StageStatus::Blocked);
        assert_eq!(
            stage.close_reason.as_deref(),
            Some("criterion 41 is unrunnable")
        );
    }

    #[test]
    fn a_block_clears_a_prior_attempts_failure_info() {
        use crate::models::failure::{FailureInfo, FailureType};

        // A crash auto-retry and a `loom stage retry` without `--force` both
        // requeue a stage with its old `failure_info` in place; a provision
        // block records `InfrastructureError` before the operator retries it.
        for prior in [FailureType::SessionCrash, FailureType::InfrastructureError] {
            let temp = TempDir::new().unwrap();
            let mut stage = Stage::new("build-api".to_string(), None);
            stage.id = "build-api".to_string();
            stage.status = StageStatus::Executing;
            stage.failure_info = Some(FailureInfo {
                failure_type: prior.clone(),
                detected_at: Utc::now(),
                evidence: vec!["an earlier attempt".to_string()],
            });
            save_stage(&stage, temp.path()).unwrap();

            let response =
                handle_block_stage(temp.path(), "build-api", "spec is ambiguous").unwrap();

            assert!(matches!(response, Response::Ok), "prior {prior:?}");
            let stage = load_stage("build-api", temp.path()).unwrap();
            assert_eq!(stage.status, StageStatus::Blocked, "prior {prior:?}");
            assert_eq!(stage.close_reason.as_deref(), Some("spec is ambiguous"));
            assert!(
                stage.failure_info.is_none(),
                "prior {prior:?} must be cleared"
            );
        }
    }

    #[test]
    fn a_refused_transition_names_the_status_it_was_refused_from() {
        let temp = TempDir::new().unwrap();
        stage_in(temp.path(), "build-api", StageStatus::Completed);

        let response = handle_block_stage(temp.path(), "build-api", "too late").unwrap();

        match response {
            Response::Error { message } => assert!(
                message.contains("build-api") && message.contains("Completed"),
                "message must name the stage and the status it is in: {message}"
            ),
            other => panic!("expected a refusal, got {other:?}"),
        }
        // The refusal must not have half-applied: no reason, no status change.
        let stage = load_stage("build-api", temp.path()).unwrap();
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(stage.close_reason.is_none());
    }

    #[test]
    fn a_stage_id_shaped_like_a_path_is_refused() {
        let temp = TempDir::new().unwrap();

        assert!(handle_block_stage(temp.path(), "../../tmp/escape", "x").is_err());
    }
}
