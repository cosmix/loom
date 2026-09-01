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

use crate::daemon::protocol::Response;
use crate::verify::transitions::update_stage;

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
