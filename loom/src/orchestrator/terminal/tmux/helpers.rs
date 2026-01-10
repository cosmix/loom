//! Helper functions for tmux operations

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::types::{TMUX_ENTER_RETRY_ATTEMPTS, TMUX_ENTER_RETRY_DELAY_MS};

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

/// Set an environment variable in a tmux session
pub fn set_tmux_environment(session_name: &str, var_name: &str, value: &str) -> Result<()> {
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
pub fn get_tmux_session_pid(session_name: &str) -> Result<Option<u32>> {
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
pub fn configure_session_for_stability(session_name: &str) -> Result<()> {
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
pub fn send_keys_debounced(session_name: &str, text: &str, debounce_ms: u64) -> Result<()> {
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
pub fn parse_tmux_timestamp(timestamp_str: &str) -> Option<DateTime<Utc>> {
    let timestamp = timestamp_str.parse::<i64>().ok()?;
    DateTime::from_timestamp(timestamp, 0)
}
