//! Utility functions for E2E tests

use anyhow::{Context, Result};
use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::transition_stage;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

/// Complete a stage following the proper state machine transitions
///
/// Transitions: Ready -> Executing -> Completed
/// Returns the completed stage
pub fn complete_stage(stage_id: &str, work_dir: &Path) -> Result<Stage> {
    // First transition to Executing (required before Completed)
    transition_stage(stage_id, StageStatus::Executing, work_dir)
        .with_context(|| format!("Failed to transition {stage_id} to Executing"))?;

    // Then transition to Completed
    transition_stage(stage_id, StageStatus::Completed, work_dir)
        .with_context(|| format!("Failed to transition {stage_id} to Completed"))
}

/// Checks if tmux is installed and available
pub fn is_tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Cleans up tmux sessions with a given prefix
///
/// This is useful for cleaning up test sessions that may have been left running
pub fn cleanup_tmux_sessions(prefix: &str) -> Result<()> {
    if !is_tmux_available() {
        return Ok(());
    }

    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let sessions = String::from_utf8_lossy(&output.stdout);
            for session_name in sessions.lines() {
                if session_name.starts_with(prefix) {
                    let _ = Command::new("tmux")
                        .args(["kill-session", "-t", session_name])
                        .output();
                }
            }
        }
    }

    Ok(())
}

/// Polls a predicate function until it returns true or timeout is reached
///
/// Useful for waiting for asynchronous operations to complete in tests
pub fn wait_for_condition<F>(predicate: F, timeout_ms: u64) -> Result<()>
where
    F: Fn() -> bool,
{
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        if predicate() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    anyhow::bail!("Timeout waiting for condition after {timeout_ms}ms")
}

/// Writes a signal file to .work/signals/{session_id}.md
pub fn create_signal_file(work_dir: &Path, session_id: &str, content: &str) -> Result<()> {
    let signals_dir = work_dir.join(".work").join("signals");
    std::fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;

    let signal_path = signals_dir.join(format!("{session_id}.md"));

    std::fs::write(&signal_path, content)
        .with_context(|| format!("Failed to write signal file: {}", signal_path.display()))?;

    Ok(())
}
