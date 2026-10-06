//! `loom request status <id> [--session <sid>]`
//! (`doc/plans/PLAN-loom-state-confinement.md` section 6).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::commands::common::resolve_work_dir;
use crate::fs::inbox::{self, read_ledger, LedgerOutcome, RequestStatus};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// `loom request status` entry point: resolve the session from `--session`
/// or `LOOM_SESSION_ID`, the scratch root from `LOOM_SCRATCH_DIR`, print one
/// line, and exit 1 only when the request is unknown.
pub fn execute(id: String, session: Option<String>, wait_secs: Option<u64>) -> Result<()> {
    let work_dir = resolve_work_dir()?;
    let session = session.or_else(|| std::env::var("LOOM_SESSION_ID").ok());
    let scratch = std::env::var_os("LOOM_SCRATCH_DIR").map(PathBuf::from);

    // In a stage worktree `.loom/work` is a symlink to the main repository's
    // state directory, and the inbox reads refuse to follow symlinks.
    let root = canonical_root(work_dir.root());

    if let Some(secs) = wait_secs {
        let waited = {
            let mut resolve = || resolve_status(&root, session.as_deref(), scratch.as_deref(), &id);
            let mut now = Instant::now;
            let mut sleep = std::thread::sleep;
            wait_for(
                &mut resolve,
                Duration::from_secs(secs),
                &mut now,
                &mut sleep,
            )?
        };
        let note = match &waited {
            Waited::Settled(ReportedStatus::Inbox(RequestStatus::Applied)) => {
                applied_note(&root, session.as_deref(), &id)?
            }
            _ => None,
        };
        println!("{}", outcome_line(&id, waited, secs, note)?);
        return Ok(());
    }

    let status = resolve_status(&root, session.as_deref(), scratch.as_deref(), &id)?;
    let (message, not_found) = format_status(&status);
    println!("{id}: {message}");
    if not_found {
        std::process::exit(1);
    }
    Ok(())
}

/// `root` with symlinks resolved; `root` itself when it cannot be resolved.
fn canonical_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// What `loom request status` reports: either a CLI ticket the relay hook
/// has not yet picked up, or whatever the daemon's inbox says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReportedStatus {
    PendingRelay,
    Inbox(RequestStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Waited {
    Settled(ReportedStatus),
    TimedOut(ReportedStatus),
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

fn wait_for(
    resolve: &mut dyn FnMut() -> Result<ReportedStatus>,
    timeout: Duration,
    now: &mut dyn FnMut() -> Instant,
    sleep: &mut dyn FnMut(Duration),
) -> Result<Waited> {
    let deadline = now() + timeout;
    loop {
        let status = resolve()?;
        if !matches!(
            &status,
            ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon | RequestStatus::Applying)
        ) {
            return Ok(Waited::Settled(status));
        }
        let current = now();
        if current >= deadline {
            return Ok(Waited::TimedOut(status));
        }
        sleep(POLL_INTERVAL.min(deadline - current));
    }
}

fn outcome_line(id: &str, waited: Waited, secs: u64, note: Option<String>) -> Result<String> {
    match waited {
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::Applied)) => match note {
            Some(note) => Ok(format!("{id}: applied: {note}")),
            None => Ok(format!("{id}: applied")),
        },
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::Refused { reason })) => {
            anyhow::bail!("request {id} was refused: {reason}")
        }
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::NotFound)) => {
            anyhow::bail!("request {id} not found")
        }
        // `resolve_status` reports PendingRelay only while the ticket file is
        // still in the scratch directory, so the ticket still applies: the
        // hook relays it after the Bash call that created it ends.
        Waited::Settled(ReportedStatus::PendingRelay) => anyhow::bail!(
            "request {id} is not relayed yet: the relay hook relays its ticket only after the Bash call that created it ends, so a wait chained into that call cannot see it; run `loom request status {id} --wait 90` again as its own Bash call and do not run the command that created the request again, which would create a second ticket the daemon refuses"
        ),
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::UnknownAfterRestart)) => anyhow::bail!(
            "request {id} is unknown after a daemon restart: check the repository state, then run the command again if the change is missing"
        ),
        Waited::TimedOut(_) => anyhow::bail!("request {id} still pending after {secs}s"),
        // `wait_for` keeps polling these two, so they settle only if its
        // loop condition changes; naming them keeps the match exhaustive.
        Waited::Settled(ReportedStatus::Inbox(
            RequestStatus::RelayedAwaitingDaemon | RequestStatus::Applying,
        )) => anyhow::bail!("request {id} is still pending"),
    }
}

fn applied_note(root: &Path, session: Option<&str>, id: &str) -> Result<Option<String>> {
    let sessions = match session {
        Some(session) => vec![session.to_string()],
        None => list_inbox_sessions(root)?,
    };
    for session in sessions {
        if let Some(record) = read_ledger(root, &session)?
            .into_iter()
            .rev()
            .find(|record| record.id == id && record.outcome == Some(LedgerOutcome::Applied))
        {
            return Ok(record.reason);
        }
    }
    Ok(None)
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
    fn a_work_dir_symlink_resolves_once_canonicalized() {
        let base = tempfile::tempdir().unwrap();
        let real = base.path().join("real");
        let id = new_request_id();
        let dir = real.join("inbox").join("session-1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();
        let link_parent = base.path().join("wt").join(".loom");
        std::fs::create_dir_all(&link_parent).unwrap();
        let link = link_parent.join("work");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert!(resolve_status(&link, Some("session-1"), None, &id).is_err());
        assert_eq!(
            resolve_status(&canonical_root(&link), Some("session-1"), None, &id).unwrap(),
            ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon)
        );
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

#[cfg(test)]
mod wait_tests;
