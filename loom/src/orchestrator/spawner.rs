//! DEPRECATED: Legacy spawner module
//!
//! This module is deprecated. Session spawning is now handled by the terminal backend
//! abstraction in `crate::orchestrator::terminal`.
//!
//! # Migration Guide
//!
//! - `spawn_session` -> Use `TerminalBackend::spawn_session()` from `terminal` module
//! - `SpawnerConfig` -> Use `OrchestratorConfig` or `BackendType` from `terminal` module
//! - `kill_session` -> Use `TerminalBackend::kill_session()`
//!
//! # Retained Functionality
//!
//! The following crash reporting functions are still exported from this module
//! until they are migrated to a dedicated crash reporting module:
//!
//! - `CrashReport`
//! - `generate_crash_report`

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
        content.push_str("## Last 100 Lines of Log\n\n");
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
}
