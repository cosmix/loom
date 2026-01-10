//! Query functions for tmux sessions (list, status)

use anyhow::{anyhow, Context, Result};
use std::process::Command;

use super::helpers::parse_tmux_timestamp;
use super::types::TmuxSessionInfo;

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
