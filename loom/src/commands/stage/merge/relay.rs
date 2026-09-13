//! Relay path for `loom stage merge --resolved`
//! (`doc/plans/PLAN-loom-state-confinement.md` section 5). In Relay mode the
//! daemon does the ancestry check and the worktree cleanup, so the CLI only
//! relays the request instead of running any of `merge_resolved`'s logic.

use std::path::Path;

use anyhow::{Context, Result};

use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode, RelaySink, StdSink};
use crate::relay::RequestKind;

use super::resolve_stage_id;

/// Dispatch `loom stage merge --resolved` by mode. `legacy` is today's
/// `merge_resolved`, invoked unchanged for Legacy and Operator mode.
pub(super) fn resolved_entry(
    stage_id: Option<String>,
    legacy: impl FnOnce(Option<String>) -> Result<()>,
) -> Result<()> {
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    resolved_entry_with_mode(stage_id, relay_mode, &cwd, &mut StdSink::default(), legacy)
}

fn resolved_entry_with_mode(
    stage_id: Option<String>,
    relay_mode: RelayMode,
    cwd: &Path,
    sink: &mut dyn RelaySink,
    legacy: impl FnOnce(Option<String>) -> Result<()>,
) -> Result<()> {
    if let RelayMode::Relay(context) = relay_mode {
        let stage_id = resolve_stage_id(stage_id, "merge --resolved <stage-id>")?;
        return emit_merge_resolved(&stage_id, &context, cwd, sink);
    }
    legacy(stage_id)
}

fn emit_merge_resolved(
    stage_id: &str,
    context: &RelayContext,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    context.check(RequestKind::MergeResolved, Some(stage_id), cwd, uid)?;
    context.emit(
        RequestKind::MergeResolved,
        serde_json::json!({}),
        "stage merge --resolved",
        false,
        sink,
    )?;
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

    #[test]
    fn relay_mode_writes_exactly_one_empty_payload_ticket_and_never_calls_legacy() {
        use crate::models::session::SessionType;
        use crate::relay::emit::test_support::context_for;

        let fixture = context_for(SessionType::Merge);
        let mut sink = VecSink::default();

        resolved_entry_with_mode(
            Some("stage-a".to_string()),
            RelayMode::Relay(fixture.context.clone()),
            &fixture.cwd,
            &mut sink,
            |_| panic!("legacy path must not run in Relay mode"),
        )
        .unwrap();

        let tickets: Vec<_> = fs::read_dir(&fixture.context.scratch_dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(tickets.len(), 1);
        let bytes = fs::read(&tickets[0]).unwrap();
        let ticket = crate::relay::Ticket::decode(&bytes).unwrap();
        assert_eq!(ticket.kind, RequestKind::MergeResolved);
        assert_eq!(ticket.payload, serde_json::json!({}));
    }

    #[test]
    fn legacy_mode_never_touches_the_scratch_directory() {
        let scratch_root = TempDir::new().unwrap();
        let scratch = scratch_root.path().join("session-1");
        fs::create_dir(&scratch).unwrap();
        fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
        let worktree = TempDir::new().unwrap();
        let mut sink = VecSink::default();
        let mut legacy_ran = false;

        resolved_entry_with_mode(
            Some("stage-a".to_string()),
            RelayMode::Operator,
            worktree.path(),
            &mut sink,
            |_| {
                legacy_ran = true;
                Ok(())
            },
        )
        .unwrap();

        assert!(legacy_ran);
        assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    }
}
