//! Hook event logging for debugging and monitoring.
//!
//! Logs hook events to `.work/hooks/events.jsonl` for debugging
//! and auditing hook execution.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use super::config::HookEvent;

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
        /// Context usage percentage at compaction time
        #[serde(skip_serializing_if = "Option::is_none")]
        context_percent: Option<f32>,
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
/// Events are appended to `.work/hooks/events.jsonl` in JSON Lines format.
/// Each line is a complete JSON object representing one event.
pub fn log_hook_event(work_dir: &Path, event: HookEventLog) -> Result<()> {
    let hooks_dir = work_dir.join("hooks");
    let events_file = hooks_dir.join("events.jsonl");

    // Ensure hooks directory exists
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir).with_context(|| {
            format!("Failed to create hooks directory: {}", hooks_dir.display())
        })?;
    }

    // Serialize event to JSON line
    let json_line = event.to_json_line()?;

    // Append to events file
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&events_file)
        .with_context(|| format!("Failed to open events file: {}", events_file.display()))?;

    writeln!(file, "{json_line}")
        .with_context(|| format!("Failed to write event: {}", events_file.display()))?;

    Ok(())
}

/// Read recent hook events from the events file
///
/// Returns the last `limit` events, or all events if limit is None.
pub fn read_recent_events(work_dir: &Path, limit: Option<usize>) -> Result<Vec<HookEventLog>> {
    let events_file = work_dir.join("hooks").join("events.jsonl");

    if !events_file.exists() {
        return Ok(Vec::new());
    }

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
