//! Helper functions for session attachment.

use std::path::Path;

use anyhow::anyhow;

use crate::models::session::SessionStatus;

/// Format session status for display
pub(crate) fn format_status(status: &SessionStatus) -> String {
    match status {
        SessionStatus::Spawning => "spawning".to_string(),
        SessionStatus::Running => "running".to_string(),
        SessionStatus::Paused => "paused".to_string(),
        SessionStatus::Completed => "completed".to_string(),
        SessionStatus::Crashed => "crashed".to_string(),
        SessionStatus::ContextExhausted => "exhausted".to_string(),
    }
}

/// Format a helpful error message for manual mode sessions
pub(crate) fn format_manual_mode_error(
    session_id: &str,
    worktree_path: Option<&std::path::PathBuf>,
    work_dir: &Path,
) -> anyhow::Error {
    let worktree_hint = match worktree_path {
        Some(path) => format!("cd {}", path.display()),
        None => "cd .worktrees/<stage-id>".to_string(),
    };

    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
    let signal_hint = signal_path.display();

    anyhow!(
        "Session '{session_id}' was created in manual mode (no backend session).\n\n\
         To work on this stage, navigate to the worktree manually:\n  \
         {worktree_hint}\n  \
         claude \"Read the signal file at {signal_hint} and execute the assigned work.\"\n"
    )
}

/// Focus a window by process ID using wmctrl or xdotool
///
/// This is the core implementation that attempts to focus a terminal window.
/// Returns the window ID on success, or None if focusing failed.
///
/// Tries wmctrl first (more reliable), then falls back to xdotool.
pub(crate) fn try_focus_window_by_pid(pid: u32) -> Option<String> {
    use std::process::Command;

    // Try wmctrl first (more reliable for window management)
    if which::which("wmctrl").is_ok() {
        // Get window list and find the one matching our PID
        if let Ok(output) = Command::new("wmctrl").arg("-l").arg("-p").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(window_pid) = parts[2].parse::<u32>() {
                        if window_pid == pid {
                            let window_id = parts[0];
                            if Command::new("wmctrl")
                                .args(["-i", "-a", window_id])
                                .output()
                                .is_ok()
                            {
                                return Some(window_id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        if let Ok(output) = Command::new("xdotool")
            .args(["search", "--pid", &pid.to_string(), "windowactivate"])
            .output()
        {
            if output.status.success() {
                return Some(format!("xdotool-{pid}"));
            }
        }
    }

    None
}
