//! Relay path for `loom stage block` (`doc/plans/PLAN-loom-state-confinement.md`
//! section 5): in Relay mode neither the daemon socket nor the worktree spool
//! is tried — the request is handed to the relay hook instead. Also carries
//! the unchanged socket-then-spool body for Legacy and Operator mode, so
//! `state.rs`'s public `block` stays a thin entry point.

use anyhow::{Context, Result};
use std::path::Path;

use crate::daemon::{current_session_id, try_send_request, user_credential, DaemonReach, Request};
use crate::fs::stage_request::StageRequest;
use crate::relay::emit::{RelayContext, RelayMode, RelaySink};
use crate::relay::RequestKind;

/// Dispatch `loom stage block` by mode. Relay mode hands the request to the
/// relay hook; Legacy and Operator mode keep today's daemon-socket-then-spool
/// path unchanged.
pub(super) fn block_with_mode(
    stage_id: String,
    reason: String,
    relay_mode: RelayMode,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    if let RelayMode::Relay(context) = relay_mode {
        return block_relayed(&stage_id, &reason, &context, cwd, sink);
    }

    let work_dir = crate::commands::common::work_dir_path()?;
    let request = Request::BlockStage {
        auth_token: user_credential(&work_dir),
        stage_id: stage_id.clone(),
        session_id: current_session_id(),
        reason: reason.clone(),
    };

    match try_send_request(&work_dir, &request)? {
        DaemonReach::Answered(response) => super::handle_block_response(&stage_id, response)?,
        DaemonReach::NotListening => {
            crate::verify::transitions::update_stage(&stage_id, &work_dir, |stage| {
                stage.try_mark_blocked()?;
                stage.close_reason = Some(reason.clone());
                stage.updated_at = chrono::Utc::now();
                Ok(())
            })?;
        }
        DaemonReach::Unreachable => return super::queue_block_request(&stage_id, &reason),
    }

    println!("Stage '{stage_id}' blocked");
    println!("Reason: {reason}");
    Ok(())
}

/// Relay a block request. `stage_id` is passed to [`RelayContext::check`] as
/// the stage argument, so a session may only relay a block for its own stage.
fn block_relayed(
    stage_id: &str,
    reason: &str,
    context: &RelayContext,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    context.check(RequestKind::Block, Some(stage_id), cwd, uid)?;

    let request = StageRequest::Block {
        reason: reason.to_string(),
    };
    let payload = serde_json::to_value(&request).context("failed to serialize block request")?;
    context.emit(RequestKind::Block, payload, "stage block", true, sink)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[derive(Default)]
    struct VecSink {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    }

    impl RelaySink for VecSink {
        fn stdout(&mut self) -> &mut dyn Write {
            &mut self.stdout
        }
        fn stderr(&mut self) -> &mut dyn Write {
            &mut self.stderr
        }
    }

    fn stage_context(scratch: &Path, worktree: &Path) -> RelayContext {
        RelayContext {
            session_id: "session-1".to_string(),
            scratch_dir: scratch.to_path_buf(),
            stage_id: Some("stage-a".to_string()),
            session_type: Some("stage".to_string()),
            worktree_path: Some(worktree.to_path_buf()),
            work_dir: None,
        }
    }

    #[test]
    fn relay_mode_writes_exactly_one_ticket_with_the_end_turn_reminder() {
        let scratch_root = TempDir::new().unwrap();
        let scratch = scratch_root.path().join("session-1");
        fs::create_dir(&scratch).unwrap();
        fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
        let worktree = TempDir::new().unwrap();
        let context = stage_context(&scratch, worktree.path());
        let mut sink = VecSink::default();

        block_with_mode(
            "stage-a".to_string(),
            "stuck".to_string(),
            RelayMode::Relay(context),
            worktree.path(),
            &mut sink,
        )
        .unwrap();

        let tickets: Vec<_> = fs::read_dir(&scratch)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(tickets.len(), 1);
        let bytes = fs::read(&tickets[0]).unwrap();
        let ticket = crate::relay::Ticket::decode(&bytes).unwrap();
        assert_eq!(ticket.kind, RequestKind::Block);
        let decoded: StageRequest = serde_json::from_value(ticket.payload).unwrap();
        assert_eq!(
            decoded,
            StageRequest::Block {
                reason: "stuck".to_string()
            }
        );

        let stderr_text = String::from_utf8(sink.stderr).unwrap();
        assert!(stderr_text.contains("End your turn after the confirmation."));
    }

    #[test]
    fn a_mismatched_stage_argument_is_refused_before_any_ticket_is_written() {
        let scratch_root = TempDir::new().unwrap();
        let scratch = scratch_root.path().join("session-1");
        fs::create_dir(&scratch).unwrap();
        fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
        let worktree = TempDir::new().unwrap();
        let context = stage_context(&scratch, worktree.path());
        let mut sink = VecSink::default();

        let error = block_with_mode(
            "some-other-stage".to_string(),
            "stuck".to_string(),
            RelayMode::Relay(context),
            worktree.path(),
            &mut sink,
        )
        .unwrap_err();

        assert!(error.to_string().contains("does not match"));
        assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    }
}
