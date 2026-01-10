//! Multi-session overview functionality.
//!
//! Functions to create and manage multi-session views.
//! For tmux: creates tmux overview sessions with multiple windows or panes.
//! For native: focuses all terminal windows sequentially.

use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

use super::{
    attach_command_for_session, try_focus_window_by_pid, window_name_for_session,
    AttachableSession, SessionBackend,
};

/// Create a tmux overview session with windows for each loom session
///
/// Each window runs the appropriate attach command for its backend:
/// - Tmux sessions: `tmux attach -t <loom-session>`
/// - Native sessions: displays info about the native session
///
/// Uses `env -u TMUX` to handle running from within another tmux session.
pub fn create_overview_session(
    sessions: &[AttachableSession],
    detach_existing: bool,
) -> Result<String> {
    let overview_name = "loom-overview";

    // Kill existing overview session if it exists (ignore errors)
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", overview_name])
        .output();

    // Create the overview session with first loom session's window
    let first = &sessions[0];
    let first_window_name = window_name_for_session(first);
    let first_attach_cmd = attach_command_for_session(first, detach_existing);

    // Use `env -u TMUX` to unset TMUX env var, allowing nested session creation
    let output = Command::new("env")
        .args([
            "-u",
            "TMUX",
            "tmux",
            "new-session",
            "-d",
            "-s",
            overview_name,
            "-n",
            &first_window_name,
            "sh",
            "-c",
            &first_attach_cmd,
        ])
        .output()
        .context("Failed to create overview session")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create overview session: {stderr}");
    }

    // Add remaining sessions as new windows
    for session in sessions.iter().skip(1) {
        let window_name = window_name_for_session(session);
        let attach_cmd = attach_command_for_session(session, detach_existing);

        let output = Command::new("tmux")
            .args([
                "new-window",
                "-t",
                overview_name,
                "-n",
                &window_name,
                "sh",
                "-c",
                &attach_cmd,
            ])
            .output()
            .with_context(|| format!("Failed to add window for {}", session.session_id))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "Warning: Failed to add window for {}: {}",
                session.session_id, stderr
            );
        }
    }

    Ok(overview_name.to_string())
}

/// Print navigation instructions for the overview session
pub fn print_overview_instructions(session_count: usize) {
    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│  loom Overview: {session_count} session(s)                              │");
    println!("│                                                         │");
    println!("│  Navigate windows:                                      │");
    println!("│    Ctrl+B then N    - Next window                       │");
    println!("│    Ctrl+B then P    - Previous window                   │");
    println!("│    Ctrl+B then 0-9  - Jump to window by number          │");
    println!("│    Ctrl+B then W    - Window list                       │");
    println!("│                                                         │");
    println!("│  Detach (exit overview): Ctrl+B then D                  │");
    println!("│  Scroll in session:      Ctrl+B then [ (exit: q)        │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Attach to the overview session (replaces current process on Unix)
pub fn attach_overview_session(overview_name: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(overview_name)
            .exec();
        Err(anyhow!("Failed to exec tmux: {error}"))
    }

    #[cfg(not(unix))]
    {
        let status = Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(overview_name)
            .status()
            .context("Failed to execute tmux command")?;

        if !status.success() {
            bail!("tmux attach failed with status: {}", status);
        }
        Ok(())
    }
}

/// Create a tmux tiled overview with all sessions visible in panes
///
/// Creates a single tmux window with split panes for each session.
/// All sessions are visible simultaneously in a grid layout.
/// Works with both tmux and native sessions.
pub fn create_tiled_overview(
    sessions: &[AttachableSession],
    layout: &str,
    detach_existing: bool,
) -> Result<String> {
    let overview_name = "loom-overview";

    // Kill existing overview session if it exists
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", overview_name])
        .output();

    // Create the overview session with first loom session
    let first = &sessions[0];
    let first_attach_cmd = attach_command_for_session(first, detach_existing);

    // Use `env -u TMUX` to unset TMUX env var, allowing nested session creation
    let output = Command::new("env")
        .args([
            "-u",
            "TMUX",
            "tmux",
            "new-session",
            "-d",
            "-s",
            overview_name,
            "sh",
            "-c",
            &first_attach_cmd,
        ])
        .output()
        .context("Failed to create tiled overview session")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create tiled overview session: {stderr}");
    }

    // Split window for remaining sessions
    for session in sessions.iter().skip(1) {
        let attach_cmd = attach_command_for_session(session, detach_existing);

        let output = Command::new("tmux")
            .args(["split-window", "-t", overview_name, "sh", "-c", &attach_cmd])
            .output()
            .with_context(|| format!("Failed to split pane for {}", session.session_id))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "Warning: Failed to add pane for {}: {}",
                session.session_id, stderr
            );
        }

        // Apply layout after each split to keep things balanced
        let tmux_layout = match layout {
            "horizontal" => "even-horizontal",
            "vertical" => "even-vertical",
            _ => "tiled",
        };
        let _ = Command::new("tmux")
            .args(["select-layout", "-t", overview_name, tmux_layout])
            .output();
    }

    Ok(overview_name.to_string())
}

/// Print navigation instructions for tiled pane view
pub fn print_tiled_instructions(session_count: usize) {
    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│  loom Tiled View: {session_count} session(s)                           │");
    println!("│                                                         │");
    println!("│  Navigate panes:                                        │");
    println!("│    Ctrl+B then Arrow  - Move to adjacent pane           │");
    println!("│    Ctrl+B then Q      - Show pane numbers               │");
    println!("│    Ctrl+B then Z      - Zoom/unzoom current pane        │");
    println!("│                                                         │");
    println!("│  Detach (exit view): Ctrl+B then D                      │");
    println!("│  Scroll in pane:     Ctrl+B then [ (exit: q)            │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Print warning when there are many sessions (panes may be small)
pub fn print_many_sessions_warning(count: usize) {
    if count > 6 {
        eprintln!("\nWarning: {count} sessions may result in small panes.");
        eprintln!("Consider using 'loom attach all --gui' for separate windows.\n");
    }
}

/// Attach to all native sessions by focusing their windows
///
/// This is the native backend equivalent of tmux overview.
/// It attempts to focus each terminal window in sequence.
pub fn attach_native_all(sessions: &[AttachableSession]) -> Result<()> {
    let native_sessions: Vec<_> = sessions.iter().filter(|s| s.is_native()).collect();

    if native_sessions.is_empty() {
        bail!("No native sessions to attach to");
    }

    print_native_instructions(native_sessions.len());

    let mut focused = 0;
    let mut failed = 0;

    for session in &native_sessions {
        if let SessionBackend::Native { pid } = &session.backend {
            let stage_display = session
                .stage_name
                .as_ref()
                .or(session.stage_id.as_ref())
                .map(|s| s.as_str())
                .unwrap_or(&session.session_id);

            if focus_window_by_pid_quiet(*pid) {
                println!("  Focused: {stage_display} (PID: {pid})");
                focused += 1;
            } else {
                eprintln!("  Could not focus: {stage_display} (PID: {pid})");
                failed += 1;
            }
        }
    }

    println!("\nFocused {focused} of {} windows.", native_sessions.len());
    if failed > 0 {
        println!("Tip: Install wmctrl or xdotool for better window focusing.");
    }

    Ok(())
}

/// Print instructions for native session attachment
pub fn print_native_instructions(session_count: usize) {
    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│  loom Native Sessions: {session_count} session(s)                       │");
    println!("│                                                         │");
    println!("│  Native sessions run in separate terminal windows.      │");
    println!("│  Attempting to focus each window...                     │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Focus a window by PID (quiet version for batch operations)
fn focus_window_by_pid_quiet(pid: u32) -> bool {
    try_focus_window_by_pid(pid).is_some()
}
