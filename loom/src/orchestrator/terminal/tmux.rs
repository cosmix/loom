//! tmux terminal backend
//!
//! Spawns Claude Code sessions in tmux sessions with stability improvements
//! based on gastown patterns (debouncing, history clearing, zombie detection).

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::{BackendType, TerminalBackend};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

/// Debounce delay between sending text and Enter key (milliseconds)
const TMUX_DEBOUNCE_MS: u64 = 200;

/// Number of retry attempts for sending Enter key
const TMUX_ENTER_RETRY_ATTEMPTS: u32 = 3;

/// Delay between Enter key retry attempts (milliseconds)
const TMUX_ENTER_RETRY_DELAY_MS: u64 = 200;

/// tmux terminal backend - spawns sessions in tmux
pub struct TmuxBackend {
    /// Prefix for tmux session names
    prefix: String,
}

impl TmuxBackend {
    /// Create a new tmux backend
    pub fn new() -> Result<Self> {
        check_tmux_available()?;
        Ok(Self {
            prefix: "loom".to_string(),
        })
    }

    /// Get the session name prefix
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Generate session name from stage ID
    fn session_name(&self, stage_id: &str) -> String {
        format!("{}-{}", self.prefix, stage_id)
    }
}

impl TerminalBackend for TmuxBackend {
    fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        let session_name = self.session_name(&stage.id);
        let worktree_path = worktree.path.to_str().ok_or_else(|| {
            anyhow!(
                "Worktree path contains invalid UTF-8: {}",
                worktree.path.display()
            )
        })?;

        // Check if session already exists (zombie-aware check)
        ensure_session_fresh(&session_name)?;

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

        // Configure session for stability
        configure_session_for_stability(&session_name)?;

        // Enable pipe-pane logging
        let log_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from(".work/logs"));
        let log_path = log_dir.join(format!("{}.log", stage.id));
        enable_pane_logging(&session_name, &log_path)?;

        // Build the initial prompt
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the signal file at {signal_path_str} and execute the assigned stage work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and context files to read."
        );

        // Derive work_dir from signal_path
        let work_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".work".to_string());

        // Set environment variables
        set_tmux_environment(&session_name, "loom_SESSION_ID", &session.id)?;
        set_tmux_environment(&session_name, "loom_STAGE_ID", &stage.id)?;
        set_tmux_environment(&session_name, "loom_WORK_DIR", &work_dir)?;

        // Build and send the claude command
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));
        let claude_command = format!("claude {escaped_prompt}");

        if let Err(e) = send_keys_debounced(&session_name, &claude_command, TMUX_DEBOUNCE_MS) {
            let _ = kill_session_by_name(&session_name);
            return Err(anyhow!("Failed to send 'claude' command: {e}"));
        }

        // Get the PID
        let pid = get_tmux_session_pid(&session_name)?;

        // Update session
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

    fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        // Use a distinct prefix for merge sessions
        let session_name = format!("{}-merge-{}", self.prefix, stage.id);
        let repo_path = repo_root.to_str().ok_or_else(|| {
            anyhow!(
                "Repository path contains invalid UTF-8: {}",
                repo_root.display()
            )
        })?;

        // Check if session already exists (zombie-aware check)
        ensure_session_fresh(&session_name)?;

        // Create tmux session in detached mode with main repository as working directory
        let create_output = Command::new("tmux")
            .args(["new-session", "-d", "-s", &session_name, "-c", repo_path])
            .output()
            .context("Failed to create tmux session for merge")?;

        if !create_output.status.success() {
            let stderr = String::from_utf8_lossy(&create_output.stderr);
            return Err(anyhow!("Failed to create tmux merge session: {stderr}"));
        }

        // Configure session for stability
        configure_session_for_stability(&session_name)?;

        // Enable pipe-pane logging
        let log_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from(".work/logs"));
        let log_path = log_dir.join(format!("merge-{}.log", stage.id));
        enable_pane_logging(&session_name, &log_path)?;

        // Build the initial prompt for merge resolution
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the signal file at {signal_path_str} and execute the assigned stage work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and context files to read."
        );

        // Derive work_dir from signal_path
        let work_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".work".to_string());

        // Set environment variables - include merge-specific variables
        set_tmux_environment(&session_name, "loom_SESSION_ID", &session.id)?;
        set_tmux_environment(&session_name, "loom_STAGE_ID", &stage.id)?;
        set_tmux_environment(&session_name, "loom_WORK_DIR", &work_dir)?;
        set_tmux_environment(&session_name, "loom_SESSION_TYPE", "merge")?;

        // Build and send the claude command
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));
        let claude_command = format!("claude {escaped_prompt}");

        if let Err(e) = send_keys_debounced(&session_name, &claude_command, TMUX_DEBOUNCE_MS) {
            let _ = kill_session_by_name(&session_name);
            return Err(anyhow!("Failed to send 'claude' command for merge: {e}"));
        }

        // Get the PID
        let pid = get_tmux_session_pid(&session_name)?;

        // Update session
        // Note: For merge sessions, we don't set worktree_path since we're in the main repo
        let mut session = session;
        session.set_tmux_session(session_name.clone());
        session.assign_to_stage(stage.id.clone());
        if let Some(pid) = pid {
            session.set_pid(pid);
        }
        session.try_mark_running()?;

        Ok(session)
    }

    fn kill_session(&self, session: &Session) -> Result<()> {
        let session_name = session
            .tmux_session
            .as_ref()
            .ok_or_else(|| anyhow!("Session has no tmux_session name"))?;

        kill_session_by_name(session_name)
    }

    fn is_session_alive(&self, session: &Session) -> Result<bool> {
        if let Some(session_name) = &session.tmux_session {
            session_is_running(session_name)
        } else {
            Ok(false)
        }
    }

    fn attach_session(&self, session: &Session) -> Result<()> {
        let session_name = session
            .tmux_session
            .as_ref()
            .ok_or_else(|| anyhow!("Session has no tmux_session name"))?;

        if session.status != SessionStatus::Running {
            bail!("Session {} is not running", session.id);
        }

        // Use tmux attach-session
        let status = Command::new("tmux")
            .args(["attach-session", "-t", session_name])
            .status()
            .context("Failed to attach to tmux session")?;

        if !status.success() {
            bail!("Failed to attach to tmux session '{session_name}'");
        }

        Ok(())
    }

    fn attach_all(&self, sessions: &[Session]) -> Result<()> {
        // For tmux, we use the tiled overview from the attach module
        // This is handled separately by the attach command
        // Here we just validate that sessions exist
        for session in sessions {
            if session.status == SessionStatus::Running {
                if let Some(session_name) = &session.tmux_session {
                    if !session_is_running(session_name)? {
                        eprintln!(
                            "Warning: Session {} tmux session '{}' not found",
                            session.id, session_name
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Tmux
    }
}

// ============================================================================
// Helper functions (moved from spawner.rs)
// ============================================================================

/// Check if tmux is available on the system
pub fn check_tmux_available() -> Result<()> {
    if which::which("tmux").is_err() {
        return Err(anyhow!(
            "tmux is not installed. Please install tmux to use tmux backend.\n\
             On Ubuntu/Debian: sudo apt-get install tmux\n\
             On macOS: brew install tmux\n\
             On Arch: sudo pacman -S tmux"
        ));
    }
    Ok(())
}

/// Information about a tmux session
#[derive(Debug, Clone, PartialEq)]
pub struct TmuxSessionInfo {
    pub name: String,
    pub created: Option<DateTime<Utc>>,
    pub attached: bool,
    pub windows: u32,
}

/// List all loom-related tmux sessions
pub fn list_tmux_sessions(prefix: &str) -> Result<Vec<TmuxSessionInfo>> {
    let output = Command::new("tmux")
        .args([
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_created}\t#{session_attached}\t#{session_windows}",
        ])
        .output();

    let output = match output {
        Ok(output) => output,
        Err(e) => return Err(anyhow!("Failed to list tmux sessions: {e}")),
    };

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

/// Check if Claude (node process) is still running in the pane
pub fn is_agent_running(session_name: &str) -> Result<bool> {
    let output = Command::new("tmux")
        .args([
            "display-message",
            "-t",
            session_name,
            "-p",
            "#{pane_current_command}",
        ])
        .output()
        .context("Failed to get pane current command")?;

    if !output.status.success() {
        return Ok(false);
    }

    let cmd = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase();
    Ok(cmd.contains("node") || cmd.contains("claude"))
}

/// Ensure session is fresh for spawning (gastown pattern)
fn ensure_session_fresh(session_name: &str) -> Result<()> {
    if session_is_running(session_name)? {
        if !is_agent_running(session_name)? {
            kill_session_by_name(session_name)?;
        } else {
            bail!("Session '{session_name}' already running with active agent");
        }
    }
    Ok(())
}

/// Kill a tmux session by name
pub fn kill_session_by_name(session_name: &str) -> Result<()> {
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

/// Set an environment variable in a tmux session
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

/// Enable pipe-pane logging for a tmux session
pub fn enable_pane_logging(session_name: &str, log_path: &Path) -> Result<()> {
    let output = Command::new("tmux")
        .args([
            "pipe-pane",
            "-t",
            session_name,
            "-o",
            &format!("cat >> {}", log_path.display()),
        ])
        .output()
        .context("Failed to enable pipe-pane logging")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: Failed to enable pipe-pane logging for {session_name}: {stderr}");
    }

    Ok(())
}

/// Configure tmux session for stability under high output
fn configure_session_for_stability(session_name: &str) -> Result<()> {
    let _ = Command::new("tmux")
        .args(["set-option", "-t", session_name, "history-limit", "100"])
        .output();

    let _ = Command::new("tmux")
        .args(["set-option", "-t", session_name, "aggressive-resize", "on"])
        .output();

    let _ = Command::new("tmux")
        .args(["set-option", "-t", session_name, "c0-change-trigger", "10"])
        .output();

    let _ = Command::new("tmux")
        .args([
            "set-option",
            "-t",
            session_name,
            "c0-change-interval",
            "100",
        ])
        .output();

    let _ = Command::new("tmux")
        .args(["set-option", "-t", session_name, "remain-on-exit", "on"])
        .output();

    Ok(())
}

/// Clear scrollback history for a tmux session
pub fn clear_session_history(session_name: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["clear-history", "-t", session_name])
        .output()
        .context("Failed to clear tmux history")?;

    if !output.status.success() {
        return Ok(());
    }

    Ok(())
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

/// Send keys to tmux with debouncing (gastown pattern)
fn send_keys_debounced(session_name: &str, text: &str, debounce_ms: u64) -> Result<()> {
    let paste_output = Command::new("tmux")
        .args(["send-keys", "-t", session_name, "-l", text])
        .output()
        .context("Failed to send text to tmux")?;

    if !paste_output.status.success() {
        let stderr = String::from_utf8_lossy(&paste_output.stderr);
        bail!("Failed to paste text: {stderr}");
    }

    std::thread::sleep(Duration::from_millis(debounce_ms));

    send_enter_with_retry(
        session_name,
        TMUX_ENTER_RETRY_ATTEMPTS,
        TMUX_ENTER_RETRY_DELAY_MS,
    )
}

/// Send Enter key with retry logic (gastown pattern)
fn send_enter_with_retry(session_name: &str, attempts: u32, retry_delay_ms: u64) -> Result<()> {
    for attempt in 1..=attempts {
        let output = Command::new("tmux")
            .args(["send-keys", "-t", session_name, "Enter"])
            .output()
            .context("Failed to send Enter to tmux")?;

        if output.status.success() {
            return Ok(());
        }

        if attempt < attempts {
            std::thread::sleep(Duration::from_millis(retry_delay_ms));
        }
    }
    bail!("Failed to send Enter after {attempts} attempts")
}

/// Parse tmux timestamp to DateTime<Utc>
fn parse_tmux_timestamp(timestamp_str: &str) -> Option<DateTime<Utc>> {
    let timestamp = timestamp_str.parse::<i64>().ok()?;
    DateTime::from_timestamp(timestamp, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmux_backend_session_name() {
        // Skip if tmux not available
        if check_tmux_available().is_err() {
            return;
        }

        let backend = TmuxBackend::new().unwrap();
        assert_eq!(backend.session_name("stage-1"), "loom-stage-1");
        assert_eq!(backend.backend_type(), BackendType::Tmux);
    }

    #[test]
    fn test_parse_tmux_timestamp() {
        let dt = parse_tmux_timestamp("1704067200");
        assert!(dt.is_some());
        assert_eq!(dt.unwrap().timestamp(), 1704067200);

        assert!(parse_tmux_timestamp("invalid").is_none());
        assert!(parse_tmux_timestamp("").is_none());
    }

    #[test]
    fn test_tmux_session_info() {
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
}
