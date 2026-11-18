//! Hook event logging for debugging and monitoring.
//!
//! Logs hook events to `.loom/work/hooks/events.jsonl` for debugging
//! and auditing hook execution.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

use super::config::HookEvent;
use crate::fs::safe_fs::safe_open_append;

const EVENTS_FILE: &str = "events.jsonl";

/// A logged hook event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEventLog {
    /// Timestamp when the event occurred
    pub timestamp: DateTime<Utc>,
    /// The stage ID
    pub stage_id: String,
    /// The session ID
    pub session_id: String,
    /// The hook event type
    pub event: String,
    /// Optional payload with event-specific data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<HookEventPayload>,
}

/// Event-specific payload data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookEventPayload {
    /// SessionStart event data
    SessionStart {
        /// PID of the Claude Code process
        #[serde(skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
    },
    /// PreCompact event data
    PreCompact {
        /// Resident context tokens at compaction time
        #[serde(skip_serializing_if = "Option::is_none")]
        context_tokens: Option<u32>,
        /// Transcript path recorded at compaction time
        #[serde(skip_serializing_if = "Option::is_none")]
        transcript_path: Option<String>,
        /// Handoff file created
        #[serde(skip_serializing_if = "Option::is_none")]
        handoff_file: Option<String>,
    },
    /// SessionEnd event data
    SessionEnd {
        /// Exit status code
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        /// Whether stage was completed
        #[serde(skip_serializing_if = "Option::is_none")]
        completed: Option<bool>,
    },
    /// Stop event data
    Stop {
        /// Error message if stop failed
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Generic payload for custom data
    Custom {
        /// Custom data as key-value pairs
        data: std::collections::HashMap<String, String>,
    },
}

impl HookEventLog {
    /// Create a new hook event log entry
    pub fn new(stage_id: &str, session_id: &str, event: HookEvent) -> Self {
        Self {
            timestamp: Utc::now(),
            stage_id: stage_id.to_string(),
            session_id: session_id.to_string(),
            event: event.to_string(),
            payload: None,
        }
    }

    /// Create with payload
    pub fn with_payload(
        stage_id: &str,
        session_id: &str,
        event: HookEvent,
        payload: HookEventPayload,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            stage_id: stage_id.to_string(),
            session_id: session_id.to_string(),
            event: event.to_string(),
            payload: Some(payload),
        }
    }

    /// Serialize to JSON line
    pub fn to_json_line(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize hook event")
    }
}

/// Log a hook event to the events file
///
/// Events are appended to `.loom/work/hooks/events.jsonl` in JSON Lines format.
/// Each line is a complete JSON object representing one event. The append is
/// locked and refuses a symlinked or hard-linked events file; the file is
/// created with mode 0600.
pub fn log_hook_event(work_dir: &Path, event: HookEventLog) -> Result<()> {
    let hooks_dir = work_dir.join("hooks");
    let events_file = hooks_dir.join(EVENTS_FILE);

    // Ensure hooks directory exists
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir).with_context(|| {
            format!("Failed to create hooks directory: {}", hooks_dir.display())
        })?;
    }

    let mut line = event.to_json_line()?;
    line.push('\n');

    // Anchored at the hooks directory, so a worktree's `.loom/work` symlink
    // above it is followed while the events file itself must be plain.
    let mut file = safe_open_append(&hooks_dir, Path::new(EVENTS_FILE))
        .with_context(|| format!("Failed to open events file: {}", events_file.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("Failed to write event: {}", events_file.display()))?;

    Ok(())
}

/// Read recent hook events from the events file
///
/// Returns the last `limit` events, or all events if limit is None.
pub fn read_recent_events(work_dir: &Path, limit: Option<usize>) -> Result<Vec<HookEventLog>> {
    let events_file = work_dir.join("hooks").join(EVENTS_FILE);

    if !events_file.exists() {
        return Ok(Vec::new());
    }

    // Uncapped, unlike `jsonl_spool::read_complete_lines`: this log only grows, so a cap would turn its size into a read failure.
    let content = fs::read_to_string(&events_file)
        .with_context(|| format!("Failed to read events file: {}", events_file.display()))?;

    let mut events: Vec<HookEventLog> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    if let Some(n) = limit {
        let len = events.len();
        if len > n {
            events = events.split_off(len - n);
        }
    }

    Ok(events)
}

/// Read events for a specific session
pub fn read_session_events(work_dir: &Path, session_id: &str) -> Result<Vec<HookEventLog>> {
    let events = read_recent_events(work_dir, None)?;
    Ok(events
        .into_iter()
        .filter(|e| e.session_id == session_id)
        .collect())
}

/// Read events for a specific stage
pub fn read_stage_events(work_dir: &Path, stage_id: &str) -> Result<Vec<HookEventLog>> {
    let events = read_recent_events(work_dir, None)?;
    Ok(events
        .into_iter()
        .filter(|e| e.stage_id == stage_id)
        .collect())
}
