//! Multi-session overview functionality.
//!
//! Functions to create and manage tmux overview sessions with multiple
//! loom sessions visible in windows or panes.

use anyhow::{anyhow, bail, Context, Result};

use super::{attach_command, window_name_for_session, AttachableSession};

/// Create a tmux overview session with windows for each loom session
///
/// Each window runs `tmux attach -t <loom-session>` to connect to the
/// actual loom session. The overview session is named "loom-overview".
///
/// Uses `env -u TMUX` to handle running from within another tmux session.
pub fn create_overview_session(
    sessions: &[AttachableSession],
    detach_existing: bool,
) -> Result<String> {
    let overview_name = "loom-overview";

    // Kill existing overview session if it exists (ignore errors)
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", overview_name])
        .output();

    // Create the overview session with first loom session's window
    let first = &sessions[0];
    let first_window_name = window_name_for_session(first);
    let first_attach_cmd = attach_command(&first.tmux_session, detach_existing);

    // Use `env -u TMUX` to unset TMUX env var, allowing nested session creation
    let output = std::process::Command::new("env")
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
        let attach_cmd = attach_command(&session.tmux_session, detach_existing);

        let output = std::process::Command::new("tmux")
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
        let error = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(overview_name)
            .exec();
        Err(anyhow!("Failed to exec tmux: {error}"))
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("tmux")
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
pub fn create_tiled_overview(
    sessions: &[AttachableSession],
    layout: &str,
    detach_existing: bool,
) -> Result<String> {
    let overview_name = "loom-overview";

    // Kill existing overview session if it exists
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", overview_name])
        .output();

    // Create the overview session with first loom session
    let first = &sessions[0];
    let first_attach_cmd = attach_command(&first.tmux_session, detach_existing);

    // Use `env -u TMUX` to unset TMUX env var, allowing nested session creation
    let output = std::process::Command::new("env")
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
        let attach_cmd = attach_command(&session.tmux_session, detach_existing);

        let output = std::process::Command::new("tmux")
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
        let _ = std::process::Command::new("tmux")
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
