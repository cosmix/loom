//! Crash reporting utilities.
//!
//! Session spawning lives in `crate::orchestrator::terminal::native`
//! (`NativeBackend`). This module retains the crash reporting types
//! until they are migrated to a dedicated crash reporting module:
//!
//! - `CrashReport`
//! - `generate_crash_report`

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Lines of captured session output embedded in a crash report. Named once
/// because the report's own heading quotes the number.
pub const CRASH_LOG_TAIL_LINES: usize = 100;

/// Bytes read from the END of a log before it is split into lines. Bounds the
/// daemon's allocation against a long-lived session that wrote to stderr for
/// hours: only the end of that file can explain how the session ended.
const MAX_LOG_READ_BYTES: u64 = 256 * 1024;

/// Bytes of tail kept in the report. A session stuck printing the same error
/// must not bury the rest of the diagnosis under its own output, and the last
/// words are the ones that matter, so the cut is made from the front.
const MAX_LOG_TAIL_BYTES: usize = 16 * 1024;

// ============================================================================
// Crash Reporting (retained functionality)
// ============================================================================

/// Content for a crash report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    /// When the crash was detected
    pub detected_at: DateTime<Utc>,
    /// Session ID that crashed
    pub session_id: String,
    /// Stage ID associated with the crash
    pub stage_id: Option<String>,
    /// Exit code if available
    pub exit_code: Option<i32>,
    /// Error message or crash reason
    pub reason: String,
    /// Last N lines from the session log
    pub log_tail: Option<String>,
    /// Path to the full session log file
    pub log_path: Option<PathBuf>,
}

impl CrashReport {
    /// Create a new crash report
    pub fn new(session_id: String, stage_id: Option<String>, reason: String) -> Self {
        Self {
            detected_at: Utc::now(),
            session_id,
            stage_id,
            exit_code: None,
            reason,
            log_tail: None,
            log_path: None,
        }
    }

    /// Set the exit code
    pub fn with_exit_code(mut self, exit_code: i32) -> Self {
        self.exit_code = Some(exit_code);
        self
    }

    /// Set the log tail from captured session output
    pub fn with_log_tail(mut self, log_tail: String) -> Self {
        self.log_tail = Some(log_tail);
        self
    }

    /// Set the path to the full log file
    pub fn with_log_path(mut self, log_path: PathBuf) -> Self {
        self.log_path = Some(log_path);
        self
    }
}

/// The last `max_lines` lines of `path`, ready to embed in a crash report.
///
/// `None` when the file is missing, unreadable, or holds nothing but
/// whitespace: the caller then leaves the report's log section as it was
/// rather than presenting an empty capture as evidence. Invalid UTF-8 is
/// replaced rather than rejected — one bad byte must not cost the operator the
/// whole diagnosis. The read itself seeks to the last `MAX_LOG_READ_BYTES`
/// by byte offset, so on a log larger than that the first line emitted here
/// can be a truncated fragment rather than a complete line; that is
/// acceptable for evidence text, where later lines matter more than the first.
pub fn read_log_tail(path: &Path, max_lines: usize) -> Option<String> {
    let bytes = read_trailing_bytes(path, MAX_LOG_READ_BYTES)?;
    let contents = String::from_utf8_lossy(&bytes);
    if contents.trim().is_empty() {
        return None;
    }

    let lines: Vec<&str> = contents.lines().collect();
    let first = lines.len().saturating_sub(max_lines);
    Some(clamp_from_front(
        lines[first..].join("\n"),
        MAX_LOG_TAIL_BYTES,
    ))
}

/// The last `max_bytes` of `path`, or the whole file when it is smaller.
fn read_trailing_bytes(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len > max_bytes {
        file.seek(SeekFrom::Start(len - max_bytes)).ok()?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// `text` reduced to at most `max_bytes` by dropping from the FRONT, landing on
/// a character boundary so the result is still valid UTF-8.
fn clamp_from_front(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut cut = text.len() - max_bytes;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    text[cut..].to_string()
}

/// Generate a crash report file in the crashes directory
///
/// Creates a markdown file with crash diagnostics including:
/// - Timestamp and session/stage info
/// - Crash reason
/// - Log tail if provided in the report
pub fn generate_crash_report(report: &CrashReport, crashes_dir: &Path) -> Result<PathBuf> {
    // Ensure crashes directory exists
    if !crashes_dir.exists() {
        std::fs::create_dir_all(crashes_dir).with_context(|| {
            format!(
                "Failed to create crashes directory: {}",
                crashes_dir.display()
            )
        })?;
    }

    // Use log tail from report if provided
    let log_tail = report.log_tail.clone();

    // Use log path from report if provided
    let log_path = report.log_path.clone();

    // Generate filename with timestamp
    let timestamp = report.detected_at.format("%Y%m%d-%H%M%S");
    let filename = if let Some(stage_id) = &report.stage_id {
        format!("{timestamp}-{stage_id}.md")
    } else {
        format!("{timestamp}-{}.md", report.session_id)
    };

    let crash_path = crashes_dir.join(&filename);

    // Build the crash report content
    let mut content = String::new();
    content.push_str("---\n");
    content.push_str(&format!(
        "detected_at: \"{}\"\n",
        report.detected_at.to_rfc3339()
    ));
    content.push_str(&format!("session_id: \"{}\"\n", report.session_id));
    if let Some(stage_id) = &report.stage_id {
        content.push_str(&format!("stage_id: \"{stage_id}\"\n"));
    }
    if let Some(code) = report.exit_code {
        content.push_str(&format!("exit_code: {code}\n"));
    }
    content.push_str(&format!(
        "reason: \"{}\"\n",
        report.reason.replace('"', "\\\"")
    ));
    if let Some(path) = &log_path {
        content.push_str(&format!("log_file: \"{}\"\n", path.display()));
    }
    content.push_str("---\n\n");

    content.push_str("# Crash Report\n\n");
    content.push_str("## Summary\n\n");
    content.push_str(&format!(
        "- **Detected**: {}\n",
        report.detected_at.to_rfc3339()
    ));
    content.push_str(&format!("- **Session**: `{}`\n", report.session_id));
    if let Some(stage_id) = &report.stage_id {
        content.push_str(&format!("- **Stage**: `{stage_id}`\n"));
    }
    if let Some(code) = report.exit_code {
        content.push_str(&format!("- **Exit Code**: {code}\n"));
    }
    content.push_str(&format!("- **Reason**: {}\n", report.reason));
    content.push('\n');

    if let Some(tail) = &log_tail {
        content.push_str(&format!("## Last {CRASH_LOG_TAIL_LINES} Lines of Log\n\n"));
        content.push_str("```\n");
        content.push_str(tail);
        if !tail.ends_with('\n') {
            content.push('\n');
        }
        content.push_str("```\n\n");
    } else {
        content.push_str("## Log Output\n\n");
        content
            .push_str("*No log output captured. Session logging may not have been enabled.*\n\n");
    }

    if let Some(path) = &log_path {
        if path.exists() {
            content.push_str("## Full Log File\n\n");
            content.push_str(&format!("See full output at: `{}`\n", path.display()));
        }
    }

    content.push_str("\n## Recovery\n\n");
    content.push_str("This stage has been marked as blocked. To retry:\n\n");
    content.push_str("1. Investigate the crash cause using the log output above\n");
    content.push_str("2. Fix any issues in the codebase or configuration\n");
    content.push_str("3. Run `loom resume <stage-id>` to retry the stage\n");

    std::fs::write(&crash_path, &content)
        .with_context(|| format!("Failed to write crash report: {}", crash_path.display()))?;

    Ok(crash_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_report_new() {
        let report = CrashReport::new(
            "session-123".to_string(),
            Some("stage-1".to_string()),
            "Process crashed".to_string(),
        );

        assert_eq!(report.session_id, "session-123");
        assert_eq!(report.stage_id, Some("stage-1".to_string()));
        assert_eq!(report.reason, "Process crashed");
        assert!(report.exit_code.is_none());
        assert!(report.log_tail.is_none());
        assert!(report.log_path.is_none());
    }

    #[test]
    fn test_crash_report_builder() {
        let report = CrashReport::new(
            "session-123".to_string(),
            Some("stage-1".to_string()),
            "Process crashed".to_string(),
        )
        .with_exit_code(1)
        .with_log_tail("last line of log".to_string())
        .with_log_path(PathBuf::from("/tmp/test.log"));

        assert_eq!(report.exit_code, Some(1));
        assert_eq!(report.log_tail, Some("last line of log".to_string()));
        assert_eq!(report.log_path, Some(PathBuf::from("/tmp/test.log")));
    }

    #[test]
    fn test_generate_crash_report() {
        let temp = tempfile::TempDir::new().unwrap();
        let crashes_dir = temp.path().join("crashes");

        let report = CrashReport::new(
            "session-123".to_string(),
            Some("stage-1".to_string()),
            "Test crash".to_string(),
        );

        let result = generate_crash_report(&report, &crashes_dir);
        assert!(result.is_ok());

        let crash_path = result.unwrap();
        assert!(crash_path.exists());

        let content = std::fs::read_to_string(&crash_path).unwrap();
        assert!(content.contains("# Crash Report"));
        assert!(content.contains("session-123"));
        assert!(content.contains("stage-1"));
        assert!(content.contains("Test crash"));
    }

    #[test]
    fn read_log_tail_returns_only_the_last_lines() {
        let temp = tempfile::TempDir::new().unwrap();
        let log = temp.path().join("session.stderr.log");
        let body: String = (1..=150).map(|n| format!("line {n}\n")).collect();
        std::fs::write(&log, body).unwrap();

        let tail = read_log_tail(&log, 100).unwrap();
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines.len(), 100);
        assert_eq!(lines.first(), Some(&"line 51"));
        assert_eq!(lines.last(), Some(&"line 150"));
    }

    /// A crash report claims a capture only when there is something to show.
    /// An empty or whitespace-only log means the wrapper created the file and
    /// claude printed nothing — reporting that as evidence would be a lie.
    #[test]
    fn read_log_tail_declines_missing_empty_and_blank_logs() {
        let temp = tempfile::TempDir::new().unwrap();
        assert!(read_log_tail(&temp.path().join("absent.log"), 100).is_none());

        let empty = temp.path().join("empty.log");
        std::fs::write(&empty, "").unwrap();
        assert!(read_log_tail(&empty, 100).is_none());

        let blank = temp.path().join("blank.log");
        std::fs::write(&blank, "\n   \n\t\n").unwrap();
        assert!(read_log_tail(&blank, 100).is_none());
    }

    #[test]
    fn read_log_tail_caps_a_runaway_log() {
        let temp = tempfile::TempDir::new().unwrap();
        let log = temp.path().join("runaway.stderr.log");
        // One long line, so the line cap cannot do the trimming and the byte
        // cap is the only thing standing between the report and the file.
        std::fs::write(&log, "x".repeat(MAX_LOG_TAIL_BYTES * 4)).unwrap();

        let tail = read_log_tail(&log, 100).unwrap();
        assert_eq!(tail.len(), MAX_LOG_TAIL_BYTES);
    }
}
