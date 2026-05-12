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

/// A tool execution event logged by the PostToolUse hook.
///
/// Written to `.work/tool-events.jsonl` by the `post-tool-use.sh` hook.
/// Fields marked `Option` are best-effort and may be null if the hook
/// payload lacked the data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEvent {
    /// ISO 8601 timestamp
    pub ts: String,
    /// Tool name (e.g., "Bash", "Read", "Edit")
    pub tool: String,
    /// Whether the tool call resulted in an error
    pub is_error: bool,
    /// Session ID that executed the tool
    pub session_id: String,
    /// Stage ID that the session belongs to
    pub stage_id: String,
    /// Exit code for Bash tool, null for other tools
    #[serde(default)]
    pub exit: Option<i32>,
    /// Byte length of the tool output
    #[serde(default)]
    pub output_bytes: Option<u64>,
    /// First ~200 bytes of output (UTF-8 safe truncation)
    #[serde(default)]
    pub output_head: Option<String>,
    /// Last ~200 bytes of output (UTF-8 safe truncation)
    #[serde(default)]
    pub output_tail: Option<String>,
}

/// Read all tool events from `.work/tool-events.jsonl`.
///
/// Returns `Ok(empty vec)` if the file does not exist.
/// Skips blank lines and malformed lines (warns via tracing).
pub fn read_tool_events(work_dir: &Path) -> std::io::Result<Vec<ToolEvent>> {
    let events_file = work_dir.join("tool-events.jsonl");

    if !events_file.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&events_file)?;

    let events = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str(line) {
            Ok(event) => Some(event),
            Err(e) => {
                tracing::warn!("Skipping malformed tool-events.jsonl line: {}", e);
                None
            }
        })
        .collect();

    Ok(events)
}

/// Read the last `n` tool events from `.work/tool-events.jsonl`.
///
/// Returns `Ok(empty vec)` if the file does not exist.
pub fn tail_tool_events(work_dir: &Path, n: usize) -> std::io::Result<Vec<ToolEvent>> {
    let mut events = read_tool_events(work_dir)?;
    let len = events.len();
    if len > n {
        events = events.split_off(len - n);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_events_file(dir: &TempDir, content: &str) {
        let path = dir.path().join("tool-events.jsonl");
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_parses_minimal_row() {
        let dir = TempDir::new().unwrap();
        write_events_file(
            &dir,
            r#"{"ts":"2026-01-01T00:00:00Z","tool":"Bash","is_error":false,"session_id":"s1","stage_id":"st1"}"#,
        );
        let events = read_tool_events(dir.path()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool, "Bash");
        assert!(!events[0].is_error);
        assert!(events[0].exit.is_none());
        assert!(events[0].output_bytes.is_none());
    }

    #[test]
    fn test_parses_full_row() {
        let dir = TempDir::new().unwrap();
        write_events_file(
            &dir,
            r#"{"ts":"2026-01-01T00:00:00Z","tool":"Bash","is_error":false,"session_id":"s1","stage_id":"st1","exit":0,"output_bytes":42,"output_head":"hello","output_tail":"world"}"#,
        );
        let events = read_tool_events(dir.path()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].exit, Some(0));
        assert_eq!(events[0].output_bytes, Some(42));
        assert_eq!(events[0].output_head.as_deref(), Some("hello"));
        assert_eq!(events[0].output_tail.as_deref(), Some("world"));
    }

    #[test]
    fn test_tail_returns_correct_slice() {
        let dir = TempDir::new().unwrap();
        let line = r#"{"ts":"2026-01-01T00:00:00Z","tool":"Bash","is_error":false,"session_id":"s1","stage_id":"st1"}"#;
        let content = format!("{}\n{}\n{}\n", line, line, line);
        write_events_file(&dir, &content);
        let events = tail_tool_events(dir.path(), 2).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let events = read_tool_events(dir.path()).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_malformed_line_is_skipped() {
        let dir = TempDir::new().unwrap();
        write_events_file(
            &dir,
            "not-valid-json\n{\"ts\":\"2026-01-01T00:00:00Z\",\"tool\":\"Read\",\"is_error\":false,\"session_id\":\"s1\",\"stage_id\":\"st1\"}\n",
        );
        let events = read_tool_events(dir.path()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool, "Read");
    }
}
