use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

/// Configuration for session spawning
#[derive(Debug, Clone)]
pub struct SpawnerConfig {
    pub max_parallel_sessions: usize,
    pub tmux_prefix: String,
    pub logs_dir: Option<std::path::PathBuf>,
}

impl Default for SpawnerConfig {
    fn default() -> Self {
        Self {
            max_parallel_sessions: 4,
            tmux_prefix: "loom".to_string(),
            logs_dir: None,
        }
    }
}

/// Information about a tmux session
#[derive(Debug, Clone, PartialEq)]
pub struct TmuxSessionInfo {
    pub name: String,
    pub created: Option<DateTime<Utc>>,
    pub attached: bool,
    pub windows: u32,
}

/// Check if tmux is available on the system
pub fn check_tmux_available() -> Result<()> {
    let output = Command::new("which")
        .arg("tmux")
        .output()
        .context("Failed to check if tmux is installed")?;

    if !output.status.success() {
        return Err(anyhow!(
            "tmux is not installed. Please install tmux to use parallel session spawning.\n\
             On Ubuntu/Debian: sudo apt-get install tmux\n\
             On macOS: brew install tmux\n\
             On Arch: sudo pacman -S tmux"
        ));
    }

    Ok(())
}

/// Generate tmux session name from stage ID and prefix
fn generate_session_name(stage_id: &str, prefix: &str) -> String {
    format!("{prefix}-{stage_id}")
}

/// Spawn a new Claude Code session in tmux for the given stage
///
/// Creates a tmux session and runs the claude command in it.
/// The `session` parameter should already have a signal file generated for it.
/// Returns the Session object with tmux_session and pid populated.
pub fn spawn_session(
    stage: &Stage,
    worktree: &Worktree,
    config: &SpawnerConfig,
    session: Session,
    signal_path: &std::path::Path,
) -> Result<Session> {
    check_tmux_available()?;

    let session_name = generate_session_name(&stage.id, &config.tmux_prefix);
    let worktree_path = worktree.path.to_str().ok_or_else(|| {
        anyhow!(
            "Worktree path contains invalid UTF-8: {}",
            worktree.path.display()
        )
    })?;

    // Check if session already exists
    if session_is_running(&session_name)? {
        return Err(anyhow!(
            "Tmux session '{session_name}' already exists. Kill it first or use a different stage ID."
        ));
    }

    // Create tmux session in detached mode with working directory
    let create_output = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &session_name,
            "-c",
            worktree_path,
        ])
        .output()
        .context("Failed to create tmux session")?;

    if !create_output.status.success() {
        let stderr = String::from_utf8_lossy(&create_output.stderr);
        return Err(anyhow!("Failed to create tmux session: {stderr}"));
    }

    // Enable tmux logging if logs_dir is configured
    if let Some(logs_dir) = &config.logs_dir {
        if let Err(e) = enable_tmux_logging(&session_name, logs_dir, &stage.id) {
            // Log the error but don't fail session creation
            eprintln!("Warning: Failed to enable tmux logging for session '{session_name}': {e}");
        }
    }

    // Build the initial prompt that instructs Claude Code to read the signal file
    let signal_path_str = signal_path.to_string_lossy();
    let initial_prompt = format!(
        "Read the signal file at {signal_path_str} and execute the assigned stage work. \
         This file contains your assignment, tasks, acceptance criteria, \
         and context files to read."
    );

    // Derive work_dir from signal_path (signal_path is .work/signals/session-xxx.md)
    let work_dir = signal_path
        .parent() // signals/
        .and_then(|p| p.parent()) // .work/
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".work".to_string());

    // Set environment variables securely using tmux set-environment
    // This avoids shell injection by not interpolating values into shell commands
    // These env vars allow the Stop hook to signal stage completion back to loom
    set_tmux_environment(&session_name, "loom_SESSION_ID", &session.id)?;
    set_tmux_environment(&session_name, "loom_STAGE_ID", &stage.id)?;
    set_tmux_environment(&session_name, "loom_WORK_DIR", &work_dir)?;

    // Build the claude command with properly escaped prompt argument
    // Use shell_escape to safely quote the prompt for shell interpretation
    let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));
    let claude_command = format!("claude {escaped_prompt}");

    let send_output = Command::new("tmux")
        .args(["send-keys", "-t", &session_name, &claude_command, "Enter"])
        .output()
        .context("Failed to send keys to tmux session")?;

    if !send_output.status.success() {
        let stderr = String::from_utf8_lossy(&send_output.stderr);
        // Clean up the session since we failed to start claude
        let _ = kill_session_by_name(&session_name);
        return Err(anyhow!("Failed to send 'claude' command: {stderr}"));
    }

    // Get the PID of the tmux session (the shell's PID)
    let pid = get_tmux_session_pid(&session_name)?;

    // Update the passed-in Session object with tmux details
    let mut session = session;
    session.set_tmux_session(session_name.clone());
    session.set_worktree_path(worktree.path.clone());
    session.assign_to_stage(stage.id.clone());
    if let Some(pid) = pid {
        session.set_pid(pid);
    }
    session.try_mark_running()?;

    Ok(session)
}

/// Get the PID of a tmux session's active pane
fn get_tmux_session_pid(session_name: &str) -> Result<Option<u32>> {
    let output = Command::new("tmux")
        .args(["list-panes", "-t", session_name, "-F", "#{pane_pid}"])
        .output()
        .context("Failed to get tmux pane PID")?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let pid_str = stdout.trim();

    if pid_str.is_empty() {
        return Ok(None);
    }

    let pid = pid_str
        .parse::<u32>()
        .context("Failed to parse PID as u32")?;

    Ok(Some(pid))
}

/// Kill a running tmux session
pub fn kill_session(session: &Session) -> Result<()> {
    let session_name = session
        .tmux_session
        .as_ref()
        .ok_or_else(|| anyhow!("Session has no tmux_session name"))?;

    kill_session_by_name(session_name)
}

/// Kill a tmux session by name
fn kill_session_by_name(session_name: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output()
        .context("Failed to kill tmux session")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to kill tmux session: {stderr}"));
    }

    Ok(())
}

/// Enable tmux logging for a session
///
/// Uses `tmux pipe-pane` to capture all session output to a log file.
/// The log file is named `{stage_id}.log` and stored in the logs directory.
fn enable_tmux_logging(
    session_name: &str,
    logs_dir: &std::path::Path,
    stage_id: &str,
) -> Result<()> {
    // Ensure logs directory exists
    if !logs_dir.exists() {
        std::fs::create_dir_all(logs_dir)
            .with_context(|| format!("Failed to create logs directory: {}", logs_dir.display()))?;
    }

    let log_path = logs_dir.join(format!("{stage_id}.log"));
    let log_path_str = log_path
        .to_str()
        .ok_or_else(|| anyhow!("Log path contains invalid UTF-8: {}", log_path.display()))?;

    // Use tmux pipe-pane to capture session output
    // The -o flag opens output pipe (as opposed to closing with empty command)
    // We use 'cat >> file' to append output to the log file
    let pipe_command = format!("cat >> {log_path_str}");

    let output = Command::new("tmux")
        .args(["pipe-pane", "-o", "-t", session_name, &pipe_command])
        .output()
        .context("Failed to enable tmux logging")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to enable tmux pipe-pane: {stderr}"));
    }

    Ok(())
}

/// Get the last N lines from a session log file
///
/// Returns the tail of the log file, useful for crash reports.
/// Returns None if the log file doesn't exist or is empty.
pub fn get_session_log_tail(
    logs_dir: &std::path::Path,
    stage_id: &str,
    lines: usize,
) -> Option<String> {
    let log_path = logs_dir.join(format!("{stage_id}.log"));

    if !log_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&log_path).ok()?;
    if content.is_empty() {
        return None;
    }

    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    let tail_lines: Vec<&str> = all_lines[start..].to_vec();

    Some(tail_lines.join("\n"))
}

/// Content for a crash report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    /// When the crash was detected
    pub detected_at: DateTime<Utc>,
    /// Session ID that crashed
    pub session_id: String,
    /// Stage ID associated with the crash
    pub stage_id: Option<String>,
    /// Tmux session name
    pub tmux_session: Option<String>,
    /// Exit code if available
    pub exit_code: Option<i32>,
    /// Error message or crash reason
    pub reason: String,
    /// Last N lines from the tmux log
    pub log_tail: Option<String>,
    /// Path to the full tmux log file
    pub log_path: Option<PathBuf>,
}

impl CrashReport {
    /// Create a new crash report
    pub fn new(session_id: String, stage_id: Option<String>, reason: String) -> Self {
        Self {
            detected_at: Utc::now(),
            session_id,
            stage_id,
            tmux_session: None,
            exit_code: None,
            reason,
            log_tail: None,
            log_path: None,
        }
    }

    /// Set the tmux session name
    pub fn with_tmux_session(mut self, tmux_session: String) -> Self {
        self.tmux_session = Some(tmux_session);
        self
    }

    /// Set the exit code
    pub fn with_exit_code(mut self, exit_code: i32) -> Self {
        self.exit_code = Some(exit_code);
        self
    }

    /// Set the log tail from captured tmux output
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
/// - Last N lines of tmux log output
/// - Path to full log file for detailed investigation
pub fn generate_crash_report(
    report: &CrashReport,
    crashes_dir: &Path,
    logs_dir: &Path,
) -> Result<PathBuf> {
    // Ensure crashes directory exists
    if !crashes_dir.exists() {
        std::fs::create_dir_all(crashes_dir).with_context(|| {
            format!(
                "Failed to create crashes directory: {}",
                crashes_dir.display()
            )
        })?;
    }

    // Get log tail if we have a stage_id and it's not already set
    let log_tail = report.log_tail.clone().or_else(|| {
        report
            .stage_id
            .as_ref()
            .and_then(|stage_id| get_session_log_tail(logs_dir, stage_id, 100))
    });

    // Determine log path
    let log_path = report.log_path.clone().or_else(|| {
        report
            .stage_id
            .as_ref()
            .map(|stage_id| logs_dir.join(format!("{stage_id}.log")))
    });

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
    if let Some(tmux) = &report.tmux_session {
        content.push_str(&format!("tmux_session: \"{tmux}\"\n"));
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
    if let Some(tmux) = &report.tmux_session {
        content.push_str(&format!("- **Tmux Session**: `{tmux}`\n"));
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
        content.push_str("*No log output captured. Tmux logging may not have been enabled.*\n\n");
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

/// Set an environment variable in a tmux session
///
/// Uses `tmux set-environment` to securely set environment variables without
/// shell interpolation, avoiding potential injection vulnerabilities.
fn set_tmux_environment(session_name: &str, var_name: &str, value: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["set-environment", "-t", session_name, var_name, value])
        .output()
        .context("Failed to set tmux environment variable")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Failed to set environment variable '{var_name}': {stderr}"
        ));
    }

    Ok(())
}

/// List all loom-related tmux sessions
///
/// Parses `tmux list-sessions` output and filters for sessions matching the prefix.
pub fn list_tmux_sessions(prefix: &str) -> Result<Vec<TmuxSessionInfo>> {
    let output = Command::new("tmux")
        .args([
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_created}\t#{session_attached}\t#{session_windows}",
        ])
        .output();

    // If tmux command fails, it might be because no sessions exist
    let output = match output {
        Ok(output) => output,
        Err(e) => return Err(anyhow!("Failed to list tmux sessions: {e}")),
    };

    // Exit code 1 typically means no server is running (no sessions)
    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = Vec::new();

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 4 {
            continue;
        }

        let name = parts[0].to_string();

        // Filter by prefix
        if !name.starts_with(prefix) {
            continue;
        }

        let created = parse_tmux_timestamp(parts[1]);
        let attached = parts[2] == "1";
        let windows = parts[3].parse::<u32>().unwrap_or(0);

        sessions.push(TmuxSessionInfo {
            name,
            created,
            attached,
            windows,
        });
    }

    Ok(sessions)
}

/// Get info about a specific tmux session
pub fn get_tmux_session_info(session_name: &str) -> Result<Option<TmuxSessionInfo>> {
    // List all sessions and filter manually since tmux -f filter syntax is complex
    let output = Command::new("tmux")
        .args([
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_created}\t#{session_attached}\t#{session_windows}",
        ])
        .output();

    let output = match output {
        Ok(output) => output,
        Err(e) => return Err(anyhow!("Failed to get tmux session info: {e}")),
    };

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 4 {
            continue;
        }

        let name = parts[0];

        // Check if this is the session we're looking for
        if name != session_name {
            continue;
        }

        let created = parse_tmux_timestamp(parts[1]);
        let attached = parts[2] == "1";
        let windows = parts[3].parse::<u32>().unwrap_or(0);

        return Ok(Some(TmuxSessionInfo {
            name: name.to_string(),
            created,
            attached,
            windows,
        }));
    }

    Ok(None)
}

/// Check if a tmux session exists and is running
pub fn session_is_running(session_name: &str) -> Result<bool> {
    let output = Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .context("Failed to check if tmux session exists")?;

    Ok(output.status.success())
}

/// Send a command to a tmux session
pub fn send_keys(session_name: &str, keys: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["send-keys", "-t", session_name, keys])
        .output()
        .context("Failed to send keys to tmux session")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to send keys: {stderr}"));
    }

    Ok(())
}

/// Parse tmux timestamp (Unix timestamp as string) to DateTime<Utc>
fn parse_tmux_timestamp(timestamp_str: &str) -> Option<DateTime<Utc>> {
    let timestamp = timestamp_str.parse::<i64>().ok()?;
    DateTime::from_timestamp(timestamp, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawner_config_default() {
        let config = SpawnerConfig::default();
        assert_eq!(config.max_parallel_sessions, 4);
        assert_eq!(config.tmux_prefix, "loom");
    }

    #[test]
    fn test_generate_session_name() {
        let name = generate_session_name("stage-1", "loom");
        assert_eq!(name, "loom-stage-1");

        let name = generate_session_name("my-stage-123", "test");
        assert_eq!(name, "test-my-stage-123");
    }

    #[test]
    fn test_parse_tmux_timestamp() {
        // Valid Unix timestamp (2024-01-01 00:00:00 UTC)
        let dt = parse_tmux_timestamp("1704067200");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.timestamp(), 1704067200);

        // Invalid timestamp
        let dt = parse_tmux_timestamp("invalid");
        assert!(dt.is_none());

        // Empty string
        let dt = parse_tmux_timestamp("");
        assert!(dt.is_none());
    }

    #[test]
    fn test_tmux_session_info_creation() {
        let info = TmuxSessionInfo {
            name: "loom-stage-1".to_string(),
            created: DateTime::from_timestamp(1704067200, 0),
            attached: true,
            windows: 2,
        };

        assert_eq!(info.name, "loom-stage-1");
        assert!(info.created.is_some());
        assert!(info.attached);
        assert_eq!(info.windows, 2);
    }

    #[test]
    fn test_spawner_config_custom() {
        let config = SpawnerConfig {
            max_parallel_sessions: 8,
            tmux_prefix: "custom".to_string(),
            logs_dir: Some(std::path::PathBuf::from("/tmp/logs")),
        };

        assert_eq!(config.max_parallel_sessions, 8);
        assert_eq!(config.tmux_prefix, "custom");
        assert_eq!(config.logs_dir, Some(std::path::PathBuf::from("/tmp/logs")));
    }

    #[test]
    fn test_get_session_log_tail_nonexistent() {
        let temp = tempfile::TempDir::new().unwrap();
        let result = get_session_log_tail(temp.path(), "nonexistent-stage", 100);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_session_log_tail_empty_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let log_path = temp.path().join("empty-stage.log");
        std::fs::write(&log_path, "").unwrap();

        let result = get_session_log_tail(temp.path(), "empty-stage", 100);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_session_log_tail_small_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let log_path = temp.path().join("test-stage.log");
        std::fs::write(&log_path, "line1\nline2\nline3").unwrap();

        let result = get_session_log_tail(temp.path(), "test-stage", 100);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
        assert!(content.contains("line3"));
    }

    #[test]
    fn test_get_session_log_tail_truncates() {
        let temp = tempfile::TempDir::new().unwrap();
        let log_path = temp.path().join("big-stage.log");
        let lines: Vec<String> = (1..=100).map(|i| format!("line{i}")).collect();
        std::fs::write(&log_path, lines.join("\n")).unwrap();

        let result = get_session_log_tail(temp.path(), "big-stage", 10);
        assert!(result.is_some());
        let content = result.unwrap();
        // Should only have the last 10 lines (line91-line100)
        assert!(!content.contains("line1\n"));
        assert!(!content.contains("line90\n"));
        assert!(content.contains("line91"));
        assert!(content.contains("line100"));
    }

    #[test]
    fn test_shell_escape_special_characters() {
        use std::borrow::Cow;

        // Test that shell metacharacters are properly escaped
        let dangerous_input = "test; rm -rf /";
        let escaped = escape(Cow::Borrowed(dangerous_input));
        // shell_escape should quote or escape the semicolon
        assert!(
            escaped.contains('\'') || escaped.contains('\\'),
            "Expected escaping for semicolon, got: {escaped}"
        );

        // Test backticks (command substitution)
        let backtick_input = "$(whoami)";
        let escaped = escape(Cow::Borrowed(backtick_input));
        assert!(
            escaped.contains('\'') || escaped.contains('\\'),
            "Expected escaping for command substitution, got: {escaped}"
        );

        // Test double quotes
        let quote_input = "test \"quoted\"";
        let escaped = escape(Cow::Borrowed(quote_input));
        assert!(
            escaped.contains('\'') || escaped.contains('\\'),
            "Expected escaping for double quotes, got: {escaped}"
        );

        // Test pipe
        let pipe_input = "test | cat /etc/passwd";
        let escaped = escape(Cow::Borrowed(pipe_input));
        assert!(
            escaped.contains('\'') || escaped.contains('\\'),
            "Expected escaping for pipe, got: {escaped}"
        );

        // Safe input should pass through (possibly quoted but not corrupted)
        let safe_input = "Read the signal file and execute";
        let escaped = escape(Cow::Borrowed(safe_input));
        // Should contain the original text
        assert!(
            escaped.contains("Read") && escaped.contains("signal"),
            "Safe input should be preserved, got: {escaped}"
        );
    }

    #[test]
    fn test_shell_escape_preserves_content() {
        use std::borrow::Cow;

        // Ensure escaping doesn't lose content
        let input =
            "Read the signal file at /path/to/signal.md and execute the assigned stage work.";
        let escaped = escape(Cow::Borrowed(input));

        // The escaped version should contain the essential content
        // (may be wrapped in quotes)
        let unquoted = escaped.trim_matches('\'');
        assert!(
            unquoted.contains("signal file") || escaped.contains("signal file"),
            "Content should be preserved after escaping"
        );
    }
}
