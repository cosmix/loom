//! `loom request status`'s answer: where one relayed request currently
//! stands (`doc/plans/PLAN-loom-state-confinement.md` section 6).

use std::path::Path;

use anyhow::{Context, Result};

use super::ledger::{read_ledger, LedgerOutcome, LedgerState};
use super::paths::{entry_path, validate_request_id};
use crate::validation::validate_id;

/// Where one relayed request currently stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestStatus {
    /// The relay hook wrote the entry file; the daemon has not drained it.
    RelayedAwaitingDaemon,
    /// The daemon is mid-application (a crash here becomes
    /// [`RequestStatus::UnknownAfterRestart`], never a silent retry).
    Applying,
    Applied,
    Refused {
        reason: String,
    },
    UnknownAfterRestart,
    /// No ledger record and no entry file: the id is unknown to this
    /// session's inbox.
    NotFound,
}

/// Resolve `id`'s status within `session_id`'s inbox. The latest ledger
/// record wins; only when none exists does an on-disk entry file mean
/// "relayed, not yet drained".
pub fn request_status(work_dir: &Path, session_id: &str, id: &str) -> Result<RequestStatus> {
    validate_id(session_id).context("invalid inbox session id")?;
    validate_request_id(id)?;

    if let Some(record) = read_ledger(work_dir, session_id)?
        .into_iter()
        .rev()
        .find(|record| record.id == id)
    {
        return Ok(match record.outcome {
            Some(LedgerOutcome::Applied) => RequestStatus::Applied,
            Some(LedgerOutcome::Refused) => RequestStatus::Refused {
                reason: record.reason.unwrap_or_default(),
            },
            Some(LedgerOutcome::UnknownAfterRestart) => RequestStatus::UnknownAfterRestart,
            None if record.state == Some(LedgerState::Applying) => RequestStatus::Applying,
            None => RequestStatus::NotFound,
        });
    }

    if entry_path(work_dir, session_id, id)?.is_file() {
        return Ok(RequestStatus::RelayedAwaitingDaemon);
    }
    Ok(RequestStatus::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::inbox::ledger::{append_ledger, LedgerRecord};
    use crate::relay::{new_request_id, RequestKind};
    use chrono::Utc;

    fn record(
        id: &str,
        state: Option<LedgerState>,
        outcome: Option<LedgerOutcome>,
    ) -> LedgerRecord {
        LedgerRecord {
            id: id.to_string(),
            kind: RequestKind::Memory,
            state,
            outcome,
            reason: None,
            at: Utc::now(),
        }
    }

    #[test]
    fn not_found_when_nothing_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        assert_eq!(
            request_status(tmp.path(), "session-1", &id).unwrap(),
            RequestStatus::NotFound
        );
    }

    #[test]
    fn relayed_awaiting_daemon_when_only_the_entry_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let dir = tmp.path().join("inbox").join("session-1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();

        assert_eq!(
            request_status(tmp.path(), "session-1", &id).unwrap(),
            RequestStatus::RelayedAwaitingDaemon
        );
    }

    #[test]
    fn applying_when_the_latest_record_has_no_outcome_yet() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id, Some(LedgerState::Applying), None),
        )
        .unwrap();

        assert_eq!(
            request_status(tmp.path(), "session-1", &id).unwrap(),
            RequestStatus::Applying
        );
    }

    #[test]
    fn applied_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id, None, Some(LedgerOutcome::Applied)),
        )
        .unwrap();

        assert_eq!(
            request_status(tmp.path(), "session-1", &id).unwrap(),
            RequestStatus::Applied
        );
    }

    #[test]
    fn refused_outcome_carries_the_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let mut refused = record(&id, None, Some(LedgerOutcome::Refused));
        refused.reason = Some("bad kind".to_string());
        append_ledger(tmp.path(), "session-1", &refused).unwrap();

        assert_eq!(
            request_status(tmp.path(), "session-1", &id).unwrap(),
            RequestStatus::Refused {
                reason: "bad kind".to_string()
            }
        );
    }

    #[test]
    fn unknown_after_restart_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id, None, Some(LedgerOutcome::UnknownAfterRestart)),
        )
        .unwrap();

        assert_eq!(
            request_status(tmp.path(), "session-1", &id).unwrap(),
            RequestStatus::UnknownAfterRestart
        );
    }

    #[test]
    fn the_latest_record_wins_over_an_earlier_one() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id, Some(LedgerState::Applying), None),
        )
        .unwrap();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id, None, Some(LedgerOutcome::Applied)),
        )
        .unwrap();

        assert_eq!(
            request_status(tmp.path(), "session-1", &id).unwrap(),
            RequestStatus::Applied
        );
    }
}
