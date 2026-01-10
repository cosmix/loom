//! Session operations for tmux backend (create, attach, kill)

use anyhow::{anyhow, bail, Context, Result};
use std::process::Command;

use super::query::{is_agent_running, session_is_running};

/// Ensure session is fresh for spawning (gastown pattern)
pub fn ensure_session_fresh(session_name: &str) -> Result<()> {
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

/// Create a new detached tmux session
pub fn create_session(session_name: &str, working_dir: &str) -> Result<()> {
    let create_output = Command::new("tmux")
        .args(["new-session", "-d", "-s", session_name, "-c", working_dir])
        .output()
        .context("Failed to create tmux session")?;

    if !create_output.status.success() {
        let stderr = String::from_utf8_lossy(&create_output.stderr);
        return Err(anyhow!("Failed to create tmux session: {stderr}"));
    }

    Ok(())
}

/// Attach to a tmux session
pub fn attach_session(session_name: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["attach-session", "-t", session_name])
        .status()
        .context("Failed to attach to tmux session")?;

    if !status.success() {
        bail!("Failed to attach to tmux session '{session_name}'");
    }

    Ok(())
}
