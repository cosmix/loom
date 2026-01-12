//! Helper functions for session attachment.

use std::path::Path;

use anyhow::anyhow;

use crate::models::session::SessionStatus;
use crate::orchestrator::terminal::native::focus_window_by_pid as focus_window_canonical;

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
/// This is a wrapper around the canonical implementation in window_ops.
/// Returns a placeholder window ID on success, or None if focusing failed.
///
/// Tries wmctrl first (more reliable), then falls back to xdotool.
pub(crate) fn try_focus_window_by_pid(pid: u32) -> Option<String> {
    // Delegate to the canonical implementation
    if focus_window_canonical(pid).is_ok() {
        // Return a placeholder window ID to maintain compatibility with existing callers
        Some(format!("window-{pid}"))
    } else {
        None
    }
}
