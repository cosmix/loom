//! `W/inbox/<session-id>/ledger.jsonl`: the daemon's at-most-once record of
//! every request it has drained from a session's inbox
//! (`doc/plans/PLAN-loom-state-confinement.md` section 8, drain steps 5-6).

use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::paths::{inbox_root_relpath, ledger_relpath, session_relpath, validate_request_id};
use crate::fs::safe_fs::{
    open_safely, safe_append_in_workdir, safe_create_dir_all_in_workdir, safe_open_dirfd,
    MAX_LOG_BYTES,
};
use crate::fs::safe_read::{is_not_found, read_to_string_bounded};
use crate::relay::RequestKind;
use crate::validation::validate_id;

/// Bound on one encoded ledger line, including its trailing newline.
/// [`append_ledger`] truncates `reason` rather than fail the append outright.
pub const MAX_LEDGER_LINE_BYTES: usize = 4096;

/// A request mid-application: recorded before the handler runs so a crash
/// between "applying" and the outcome shows up as
/// [`LedgerOutcome::UnknownAfterRestart`] rather than a silent retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LedgerState {
    Applying,
}

/// How a drained request was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LedgerOutcome {
    Applied,
    Refused,
    UnknownAfterRestart,
}

/// One `ledger.jsonl` line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerRecord {
    pub id: String,
    pub kind: RequestKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<LedgerState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<LedgerOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub at: DateTime<Utc>,
}

/// Append `record` to `session_id`'s ledger, fsynced before returning.
pub fn append_ledger(work_dir: &Path, session_id: &str, record: &LedgerRecord) -> Result<()> {
    validate_id(session_id).context("invalid inbox session id")?;
    let line = encode_ledger_line(record)?;

    let dirfd = safe_open_dirfd(work_dir)?;
    let raw = dirfd.as_raw_fd();
    safe_create_dir_all_in_workdir(raw, &inbox_root_relpath(), 0o700)?;
    safe_create_dir_all_in_workdir(raw, &session_relpath(session_id)?, 0o700)?;
    let relpath = ledger_relpath(session_id)?;
    safe_append_in_workdir(raw, &relpath, line.as_bytes())?;
    fsync_relpath(raw, &relpath)
}

fn fsync_relpath(raw: RawFd, relpath: &Path) -> Result<()> {
    let fd = open_safely(raw, relpath, libc::O_RDONLY, 0)
        .with_context(|| format!("inbox: failed to reopen {} for fsync", relpath.display()))?;
    // SAFETY: `fd` was just opened and is valid for the duration of this call.
    if unsafe { libc::fsync(fd.as_raw_fd()) } < 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("inbox: fsync failed on {}", relpath.display()));
    }
    Ok(())
}

fn encode_ledger_line(record: &LedgerRecord) -> Result<String> {
    let full = serde_json::to_string(record).context("ledger record did not serialize")?;
    if full.len() < MAX_LEDGER_LINE_BYTES {
        return Ok(format!("{full}\n"));
    }
    let Some(reason) = record.reason.clone() else {
        bail!(
            "ledger record for {} exceeds {MAX_LEDGER_LINE_BYTES} bytes with no reason to truncate",
            record.id
        );
    };
    let mut budget = reason.len();
    loop {
        let mut candidate = record.clone();
        candidate.reason = Some(truncate_to_byte_budget(&reason, budget));
        let line = serde_json::to_string(&candidate).context("ledger record did not serialize")?;
        if line.len() < MAX_LEDGER_LINE_BYTES {
            return Ok(format!("{line}\n"));
        }
        if budget == 0 {
            bail!(
                "ledger record for {} exceeds {MAX_LEDGER_LINE_BYTES} bytes even with an empty reason",
                record.id
            );
        }
        budget /= 2;
    }
}

fn truncate_to_byte_budget(s: &str, budget: usize) -> String {
    if s.len() <= budget {
        return s.to_string();
    }
    let mut end = budget;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Read every ledger record for `session_id`, tolerating a torn last line (a
/// crash mid-append). A missing ledger file is an empty session, not an
/// error.
pub fn read_ledger(work_dir: &Path, session_id: &str) -> Result<Vec<LedgerRecord>> {
    validate_id(session_id).context("invalid inbox session id")?;
    let relpath = ledger_relpath(session_id)?;
    let content = match read_to_string_bounded(work_dir, &relpath, MAX_LOG_BYTES) {
        Ok(content) => content,
        Err(error) if is_not_found(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut records = Vec::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LedgerRecord>(line) {
            Ok(record) => records.push(record),
            Err(_) if lines.peek().is_none() => {} // torn last line: tolerated
            Err(error) => return Err(error).context("ledger line did not parse as JSON"),
        }
    }
    Ok(records)
}

/// Whether `id` already has a ledger record for `session_id` — the
/// at-most-once guard the writer and the drain both check before acting.
pub fn is_recorded(work_dir: &Path, session_id: &str, id: &str) -> Result<bool> {
    validate_request_id(id)?;
    let records = read_ledger(work_dir, session_id)?;
    Ok(records.iter().any(|record| record.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::new_request_id;

    fn record(id: &str, outcome: Option<LedgerOutcome>) -> LedgerRecord {
        LedgerRecord {
            id: id.to_string(),
            kind: RequestKind::Memory,
            state: None,
            outcome,
            reason: None,
            at: Utc::now(),
        }
    }

    #[test]
    fn round_trips_two_records() {
        let tmp = tempfile::tempdir().unwrap();
        let id_a = new_request_id();
        let id_b = new_request_id();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id_a, Some(LedgerOutcome::Applied)),
        )
        .unwrap();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id_b, Some(LedgerOutcome::Refused)),
        )
        .unwrap();

        let records = read_ledger(tmp.path(), "session-1").unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, id_a);
        assert_eq!(records[1].id, id_b);
    }

    #[test]
    fn read_ledger_is_empty_for_a_missing_session() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_ledger(tmp.path(), "session-1").unwrap().is_empty());
    }

    #[test]
    fn read_ledger_tolerates_a_torn_last_line() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id, Some(LedgerOutcome::Applied)),
        )
        .unwrap();

        let ledger_path = tmp
            .path()
            .join("inbox")
            .join("session-1")
            .join("ledger.jsonl");
        let mut content = std::fs::read_to_string(&ledger_path).unwrap();
        content.push_str("{\"id\":\"truncated"); // no trailing newline, malformed JSON
        std::fs::write(&ledger_path, content).unwrap();

        let records = read_ledger(tmp.path(), "session-1").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, id);
    }

    #[test]
    fn is_recorded_matches_only_its_own_id() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        append_ledger(
            tmp.path(),
            "session-1",
            &record(&id, Some(LedgerOutcome::Applied)),
        )
        .unwrap();

        assert!(is_recorded(tmp.path(), "session-1", &id).unwrap());
        assert!(!is_recorded(tmp.path(), "session-1", &new_request_id()).unwrap());
    }

    #[test]
    fn append_ledger_truncates_an_oversized_reason_instead_of_failing() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let mut long_reason = record(&id, Some(LedgerOutcome::Refused));
        long_reason.reason = Some("x".repeat(10_000));

        append_ledger(tmp.path(), "session-1", &long_reason).unwrap();

        let records = read_ledger(tmp.path(), "session-1").unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].reason.as_ref().unwrap().len() < 10_000);
    }
}
