//! `loom request status <id> [--session <sid>]`
//! (`doc/plans/PLAN-loom-state-confinement.md` section 6).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::commands::common::resolve_work_dir;
use crate::fs::inbox::{self, RequestStatus};

/// `loom request status` entry point: resolve the session from `--session`
/// or `LOOM_SESSION_ID`, the scratch root from `LOOM_SCRATCH_DIR`, print one
/// line, and exit 1 only when the request is unknown.
pub fn execute(id: String, session: Option<String>) -> Result<()> {
    let work_dir = resolve_work_dir()?;
    let session = session.or_else(|| std::env::var("LOOM_SESSION_ID").ok());
    let scratch = std::env::var_os("LOOM_SCRATCH_DIR").map(PathBuf::from);

    let status = resolve_status(work_dir.root(), session.as_deref(), scratch.as_deref(), &id)?;
    let (message, not_found) = format_status(&status);
    println!("{id}: {message}");
    if not_found {
        std::process::exit(1);
    }
    Ok(())
}

/// What `loom request status` reports: either a CLI ticket the relay hook
/// has not yet picked up, or whatever the daemon's inbox says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReportedStatus {
    PendingRelay,
    Inbox(RequestStatus),
}

/// Pure resolution: no environment reads, so tests supply every input.
fn resolve_status(
    work_dir: &Path,
    session: Option<&str>,
    scratch: Option<&Path>,
    id: &str,
) -> Result<ReportedStatus> {
    inbox::validate_request_id(id)?;

    if let Some(scratch) = scratch {
        if scratch.join(format!("{id}.req")).is_file() {
            return Ok(ReportedStatus::PendingRelay);
        }
    }

    if let Some(session) = session {
        return Ok(ReportedStatus::Inbox(inbox::request_status(
            work_dir, session, id,
        )?));
    }

    for session_id in list_inbox_sessions(work_dir)? {
        let status = inbox::request_status(work_dir, &session_id, id)?;
        if status != RequestStatus::NotFound {
            return Ok(ReportedStatus::Inbox(status));
        }
    }
    Ok(ReportedStatus::Inbox(RequestStatus::NotFound))
}

/// Every session directory currently under `W/inbox/`, in no particular
/// order — the caller only needs to try each one.
fn list_inbox_sessions(work_dir: &Path) -> Result<Vec<String>> {
    let root = inbox::inbox_root(work_dir);
    let read_dir = match std::fs::read_dir(&root) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to list {}", root.display()))
        }
    };
    let mut sessions = Vec::new();
    for entry in read_dir {
        let entry = entry.with_context(|| format!("failed to read entry in {}", root.display()))?;
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            sessions.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(sessions)
}

/// The line to print, and whether the exit code should be 1.
fn format_status(status: &ReportedStatus) -> (String, bool) {
    match status {
        ReportedStatus::PendingRelay => (
            "pending relay (ticket not yet picked up)".to_string(),
            false,
        ),
        ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon) => {
            ("relayed, waiting for the daemon".to_string(), false)
        }
        ReportedStatus::Inbox(RequestStatus::Applying) => ("applying".to_string(), false),
        ReportedStatus::Inbox(RequestStatus::Applied) => ("applied".to_string(), false),
        ReportedStatus::Inbox(RequestStatus::Refused { reason }) => {
            (format!("refused: {reason}"), false)
        }
        ReportedStatus::Inbox(RequestStatus::UnknownAfterRestart) => {
            ("unknown after restart".to_string(), false)
        }
        ReportedStatus::Inbox(RequestStatus::NotFound) => ("not found".to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::new_request_id;

    #[test]
    fn a_pending_ticket_wins_over_the_inbox() {
        let work_dir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let id = new_request_id();
        std::fs::write(scratch.path().join(format!("{id}.req")), b"{}").unwrap();

        let status = resolve_status(
            work_dir.path(),
            Some("session-1"),
            Some(scratch.path()),
            &id,
        )
        .unwrap();
        assert_eq!(status, ReportedStatus::PendingRelay);
    }

    #[test]
    fn an_explicit_session_is_looked_up_directly() {
        let work_dir = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let dir = work_dir.path().join("inbox").join("session-1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();

        let status = resolve_status(work_dir.path(), Some("session-1"), None, &id).unwrap();
        assert_eq!(
            status,
            ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon)
        );
    }

    #[test]
    fn without_a_session_every_inbox_is_scanned() {
        let work_dir = tempfile::tempdir().unwrap();
        let id = new_request_id();
        std::fs::create_dir_all(work_dir.path().join("inbox").join("session-1")).unwrap();
        let dir = work_dir.path().join("inbox").join("session-2");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();

        let status = resolve_status(work_dir.path(), None, None, &id).unwrap();
        assert_eq!(
            status,
            ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon)
        );
    }

    #[test]
    fn not_found_when_no_session_has_it() {
        let work_dir = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let status = resolve_status(work_dir.path(), None, None, &id).unwrap();
        assert_eq!(status, ReportedStatus::Inbox(RequestStatus::NotFound));
    }

    #[test]
    fn an_invalid_id_is_refused() {
        let work_dir = tempfile::tempdir().unwrap();
        assert!(resolve_status(work_dir.path(), None, None, "not-hex").is_err());
    }

    #[test]
    fn format_status_reports_not_found_with_exit_1() {
        let (message, not_found) = format_status(&ReportedStatus::Inbox(RequestStatus::NotFound));
        assert_eq!(message, "not found");
        assert!(not_found);
    }

    #[test]
    fn format_status_reports_every_other_state_with_exit_0() {
        let cases = [
            ReportedStatus::PendingRelay,
            ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon),
            ReportedStatus::Inbox(RequestStatus::Applying),
            ReportedStatus::Inbox(RequestStatus::Applied),
            ReportedStatus::Inbox(RequestStatus::Refused {
                reason: "bad kind".to_string(),
            }),
            ReportedStatus::Inbox(RequestStatus::UnknownAfterRestart),
        ];
        for case in cases {
            let (_, not_found) = format_status(&case);
            assert!(!not_found, "{case:?} should exit 0");
        }
    }
}
