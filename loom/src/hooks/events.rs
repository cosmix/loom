//! Hook event logging for debugging and monitoring.
//!
//! Logs hook events to `.work/hooks/events.jsonl` for debugging
//! and auditing hook execution.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::config::HookEvent;

/// Number of trailing bytes scanned by [`tail_tool_events`] when the file is
/// larger than this. Sized to comfortably hold several hundred event lines
/// (each ~0.3-0.7 KiB) so that, even with multiple concurrent sessions
/// interleaving their rows, the per-session window in
/// [`crate::orchestrator::monitor::tool_analysis`] still sees a useful slice.
const TAIL_SCAN_BYTES: u64 = 256 * 1024;

/// Size threshold above which [`enforce_tool_events_retention`] truncates
/// `tool-events.jsonl` (and siblings) down to their trailing
/// [`RETENTION_KEEP_BYTES`]. 8 MiB keeps weeks of single-session activity
/// well below the unbounded-growth failure mode while leaving ample history.
const RETENTION_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Number of trailing bytes preserved when retention rotates a JSONL file.
const RETENTION_KEEP_BYTES: u64 = 2 * 1024 * 1024;

/// Names of the append-only JSONL telemetry files subject to retention.
const RETENTION_FILES: &[&str] = &["tool-events.jsonl"];

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
/// This is a **true tail**: rather than reading and parsing the entire
/// (unbounded, append-only) file, it `seek`s to at most the last
/// [`TAIL_SCAN_BYTES`] and parses only the complete lines found there. This
/// keeps the per-tick monitor cost bounded regardless of how large the file
/// has grown. The seek-to-end technique mirrors the daemon log tailer in
/// `daemon/server/broadcast.rs`.
///
/// Returns `Ok(empty vec)` if the file does not exist.
///
/// NOTE: the tail spans ALL sessions interleaved in the file, so callers that
/// want the last `n` events for a *single* session
/// (e.g. [`crate::orchestrator::monitor::tool_analysis::analyze_session`])
/// see a window diluted by concurrent sessions. [`TAIL_SCAN_BYTES`] is sized
/// large enough that the per-session slice remains useful in practice; reading
/// strictly `n` events *per session* would require either a per-session file
/// or a full scan, neither of which is worth the cost on the 5s tick path.
pub fn tail_tool_events(work_dir: &Path, n: usize) -> std::io::Result<Vec<ToolEvent>> {
    let events_file = work_dir.join("tool-events.jsonl");

    let mut file = match File::open(&events_file) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let file_len = file.metadata()?.len();
    if file_len == 0 {
        return Ok(Vec::new());
    }

    // Read only the trailing window of the file.
    let scan_bytes = file_len.min(TAIL_SCAN_BYTES);
    let seeked_into_middle = scan_bytes < file_len;
    file.seek(SeekFrom::End(-(scan_bytes as i64)))?;

    let mut buf = Vec::with_capacity(scan_bytes as usize);
    file.take(scan_bytes).read_to_end(&mut buf)?;

    // The buffer is JSONL; lossy decode tolerates a split multi-byte char at
    // the start (the first partial line is discarded below regardless).
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();

    // When we seeked into the middle of the file the first "line" is almost
    // certainly a fragment of a row that started before the window — drop it.
    if seeked_into_middle && !lines.is_empty() {
        lines.remove(0);
    }

    let mut events: Vec<ToolEvent> = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str(line) {
            Ok(event) => Some(event),
            Err(e) => {
                tracing::warn!("Skipping malformed tool-events.jsonl line: {}", e);
                None
            }
        })
        .collect();

    let len = events.len();
    if len > n {
        events = events.split_off(len - n);
    }
    Ok(events)
}

/// Enforce a size-based retention policy on the append-only tool-events
/// telemetry files under `work_dir`.
///
/// For each retained file that exceeds [`RETENTION_MAX_BYTES`], the trailing
/// [`RETENTION_KEEP_BYTES`] are preserved (aligned to the next line boundary so
/// no partial row survives) and the rest is discarded by rewriting the file.
/// Intended to be called once at daemon startup; cheap and idempotent when the
/// files are already small. Errors on individual files are logged and skipped
/// so retention never aborts startup.
pub fn enforce_tool_events_retention(work_dir: &Path) {
    for name in RETENTION_FILES {
        let path = work_dir.join(name);
        if let Err(e) = truncate_jsonl_to_tail(&path, RETENTION_MAX_BYTES, RETENTION_KEEP_BYTES) {
            tracing::warn!("Failed to enforce retention on {}: {}", path.display(), e);
        }
    }
}

/// Rewrite `path` keeping only its trailing `keep_bytes` (line-aligned) when it
/// exceeds `max_bytes`. No-op if the file is missing or already small enough.
fn truncate_jsonl_to_tail(path: &Path, max_bytes: u64, keep_bytes: u64) -> std::io::Result<()> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    let file_len = file.metadata()?.len();
    if file_len <= max_bytes {
        return Ok(());
    }

    let keep = keep_bytes.min(file_len);
    file.seek(SeekFrom::End(-(keep as i64)))?;
    let mut buf = Vec::with_capacity(keep as usize);
    file.take(keep).read_to_end(&mut buf)?;

    // Drop everything up to and including the first newline so the retained
    // file begins on a clean row boundary (the seek lands mid-row).
    let start = match buf.iter().position(|&b| b == b'\n') {
        Some(idx) => idx + 1,
        None => 0,
    };
    let kept = &buf[start..];

    // Rewrite atomically via a temp file + rename so a crash mid-write can't
    // leave a half-written telemetry file.
    let tmp_path = path.with_extension("jsonl.tmp");
    {
        let mut tmp = File::create(&tmp_path)?;
        tmp.write_all(kept)?;
        tmp.flush()?;
    }
    fs::rename(&tmp_path, path)?;
    Ok(())
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
    fn test_tail_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let events = tail_tool_events(dir.path(), 50).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_tail_returns_newest_when_more_than_n() {
        let dir = TempDir::new().unwrap();
        // 10 distinct rows; tail of 3 must be the last 3 written.
        let mut content = String::new();
        for i in 0..10 {
            content.push_str(&format!(
                r#"{{"ts":"2026-01-01T00:00:0{}Z","tool":"Bash","is_error":false,"session_id":"s1","stage_id":"st1","output_bytes":{}}}"#,
                i % 10,
                i
            ));
            content.push('\n');
        }
        write_events_file(&dir, &content);
        let events = tail_tool_events(dir.path(), 3).unwrap();
        assert_eq!(events.len(), 3);
        // Last three rows had output_bytes 7,8,9.
        assert_eq!(events[0].output_bytes, Some(7));
        assert_eq!(events[2].output_bytes, Some(9));
    }

    #[test]
    fn test_tail_seeks_into_middle_drops_partial_first_line() {
        // Build a file larger than TAIL_SCAN_BYTES so the seek lands mid-row,
        // then confirm we never surface a corrupted (partial) first row and
        // that the newest rows are returned intact.
        let dir = TempDir::new().unwrap();
        let mut content = String::new();
        let mut idx: u64 = 0;
        while (content.len() as u64) < TAIL_SCAN_BYTES + 64 * 1024 {
            content.push_str(&format!(
                r#"{{"ts":"2026-01-01T00:00:00Z","tool":"Bash","is_error":false,"session_id":"s1","stage_id":"st1","output_bytes":{idx}}}"#
            ));
            content.push('\n');
            idx += 1;
        }
        write_events_file(&dir, &content);

        let events = tail_tool_events(dir.path(), 5).unwrap();
        assert_eq!(events.len(), 5);
        // Newest row is idx-1; all five parsed cleanly (no partial-line panic).
        assert_eq!(events[4].output_bytes, Some(idx - 1));
    }

    #[test]
    fn test_retention_truncates_oversized_file() {
        let dir = TempDir::new().unwrap();
        // Write a file well over RETENTION_MAX_BYTES.
        let mut content = String::new();
        let mut idx: u64 = 0;
        while (content.len() as u64) < RETENTION_MAX_BYTES + 1024 * 1024 {
            content.push_str(&format!(
                r#"{{"ts":"2026-01-01T00:00:00Z","tool":"Bash","is_error":false,"session_id":"s1","stage_id":"st1","output_bytes":{idx}}}"#
            ));
            content.push('\n');
            idx += 1;
        }
        write_events_file(&dir, &content);
        let newest = idx - 1;

        enforce_tool_events_retention(dir.path());

        let path = dir.path().join("tool-events.jsonl");
        let new_len = std::fs::metadata(&path).unwrap().len();
        assert!(
            new_len <= RETENTION_KEEP_BYTES,
            "file should be truncated to tail"
        );

        // The newest row must survive and the file must still parse cleanly
        // (begins on a row boundary — no leading partial row).
        let events = read_tool_events(dir.path()).unwrap();
        assert!(!events.is_empty());
        assert_eq!(events.last().unwrap().output_bytes, Some(newest));
    }

    #[test]
    fn test_retention_noop_when_small() {
        let dir = TempDir::new().unwrap();
        let line = r#"{"ts":"2026-01-01T00:00:00Z","tool":"Bash","is_error":false,"session_id":"s1","stage_id":"st1"}"#;
        let content = format!("{line}\n{line}\n");
        write_events_file(&dir, &content);

        enforce_tool_events_retention(dir.path());

        // Unchanged: both rows still present.
        let events = read_tool_events(dir.path()).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_retention_missing_file_is_noop() {
        let dir = TempDir::new().unwrap();
        // No file exists — must not error.
        enforce_tool_events_retention(dir.path());
        assert!(!dir.path().join("tool-events.jsonl").exists());
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
