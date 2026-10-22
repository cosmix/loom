//! Durable log of stage status changes, `<work_dir>/stage-events.jsonl`.
//!
//! Every status change made through `update_stage` appends one line, so the
//! dashboard can show what happened before it connected. Storage is the shared
//! bounded JSONL spool: once the file reaches the spool cap, further appends
//! fail (and are logged by the caller) instead of growing without limit.
//! Revisit with rotation if a plan's history legitimately outgrows the cap.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::jsonl_spool;
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
            reason,
        }
    }
}

/// Append `event` to the log under `work_dir`.
pub fn append_event(work_dir: &Path, event: &StageEvent) -> anyhow::Result<()> {
    jsonl_spool::append(work_dir, EVENTS_FILE, event, RECORD_LIMIT)
}

/// The last `limit` events in file order; a missing or unreadable log is empty
/// and unparseable lines are skipped.
pub fn read_recent(work_dir: &Path, limit: usize) -> Vec<StageEvent> {
    let mut events: Vec<StageEvent> = jsonl_spool::read(work_dir, EVENTS_FILE).unwrap_or_default();
    events.drain(..events.len().saturating_sub(limit));
    events
}

#[cfg(test)]
#[path = "stage_events_tests.rs"]
mod tests;
