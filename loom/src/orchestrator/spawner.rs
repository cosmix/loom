use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use std::process::Command;

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

/// Configuration for session spawning
#[derive(Debug, Clone)]
pub struct SpawnerConfig {
    pub max_parallel_sessions: usize,
    pub tmux_prefix: String,
}

impl Default for SpawnerConfig {
    fn default() -> Self {
        Self {
            max_parallel_sessions: 4,
            tmux_prefix: "loom".to_string(),
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

    // Build the initial prompt that instructs Claude Code to read the signal file
    let initial_prompt = format!(
        "Read the signal file at {} and execute the assigned stage work. \
         This file contains your assignment, tasks, acceptance criteria, \
         and context files to read.",
        signal_path.display()
    );

    // Derive work_dir from signal_path (signal_path is .work/signals/session-xxx.md)
    let work_dir = signal_path
        .parent() // signals/
        .and_then(|p| p.parent()) // .work/
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".work".to_string());

    // Send the claude command with environment variables for hook integration
    // These env vars allow the Stop hook to signal stage completion back to loom
    let claude_command = format!(
        "export loom_SESSION_ID='{}' loom_STAGE_ID='{}' loom_WORK_DIR='{}'; claude \"{}\"",
        session.id,
        stage.id,
        work_dir,
        initial_prompt.replace('"', "\\\"")
    );
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
    session.mark_running();

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
        };

        assert_eq!(config.max_parallel_sessions, 8);
        assert_eq!(config.tmux_prefix, "custom");
    }
}
