//! Durable log of stage status changes, `<work_dir>/stage-events.jsonl`.
//!
//! Every status or executing-session change through `update_stage` appends one line, so the
//! dashboard can show what happened before it connected. Storage is the shared
//! bounded JSONL spool: once the file reaches the spool cap, further appends
//! fail (and are logged by the caller) instead of growing without limit.
//! Revisit with rotation if a plan's history legitimately outgrows the cap.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::jsonl_spool;
use crate::models::session::SessionType;
use crate::models::stage::StageStatus;

const EVENTS_FILE: &str = "stage-events.jsonl";
/// Events shipped in the status data.
pub const RECENT_LIMIT: usize = 50;
const RECORD_LIMIT: usize = 4096;
const REASON_CHARS: usize = 500;

/// One stage status change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageEvent {
    pub at: DateTime<Utc>,
    pub stage_id: String,
    pub from: StageStatus,
    pub to: StageStatus,
    /// The stage's session after the change, when it has one.
    pub session: Option<String>,
    /// Session kind at the event time; absent in older logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_type: Option<SessionType>,
    /// The close reason, only when the stage became blocked.
    pub reason: Option<String>,
}

impl StageEvent {
    /// Build the event for a change of `stage_id` from `from` to `to`.
    pub fn new(
        stage_id: &str,
        from: StageStatus,
        to: StageStatus,
        session: Option<String>,
        close_reason: Option<&str>,
    ) -> Self {
        let reason = (to == StageStatus::Blocked)
            .then_some(close_reason)
            .flatten()
            .map(|reason| reason.chars().take(REASON_CHARS).collect());
        Self {
            at: Utc::now(),
            stage_id: stage_id.to_owned(),
            from,
            to,
            session,
            session_type: None,
            reason,
        }
    }
}

/// `work_dir` with its own symlinks resolved. Inside a worktree `.loom/work` is
/// a trusted symlink to the shared state directory, and the spool refuses a
/// symlink as the root's last component. The resolved directory is the root; the
/// log file and anything below it are still opened without following a link.
/// An unresolvable path is passed through so the spool reports the failure.
fn state_root(work_dir: &Path) -> PathBuf {
    std::fs::canonicalize(work_dir).unwrap_or_else(|_| work_dir.to_path_buf())
}

/// Append `event` to the log under `work_dir`.
pub fn append_event(work_dir: &Path, event: &StageEvent) -> anyhow::Result<()> {
    jsonl_spool::append(&state_root(work_dir), EVENTS_FILE, event, RECORD_LIMIT)
}

/// The last `limit` events in file order; a missing or unreadable log is empty
/// and unparseable lines are skipped.
pub fn read_recent(work_dir: &Path, limit: usize) -> Vec<StageEvent> {
    let mut events: Vec<StageEvent> =
        jsonl_spool::read(&state_root(work_dir), EVENTS_FILE).unwrap_or_default();
    events.drain(..events.len().saturating_sub(limit));
    events
}

#[cfg(test)]
#[path = "stage_events_tests.rs"]
mod tests;
