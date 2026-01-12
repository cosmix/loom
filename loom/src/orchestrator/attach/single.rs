//! Single session attachment functionality.
//!
//! Functions to attach to individual sessions by stage ID or session ID.
//! Supports native terminal backends.

use std::path::Path;

use anyhow::{anyhow, Result};

use super::{detect_backend_type, find_session_for_stage, format_manual_mode_error, load_session};

/// Attach to a session by stage ID
///
/// - Looks up the session for the stage
/// - For native: attempts to focus the terminal window
pub fn attach_by_stage(stage_id: &str, work_dir: &Path) -> Result<()> {
    let session = find_session_for_stage(work_dir, stage_id)?
        .ok_or_else(|| anyhow!("No active session found for stage '{stage_id}'"))?;

    let _backend_type = detect_backend_type(&session).ok_or_else(|| {
        format_manual_mode_error(&session.id, session.worktree_path.as_ref(), work_dir)
    })?;

    let pid = session.pid.ok_or_else(|| {
        format_manual_mode_error(&session.id, session.worktree_path.as_ref(), work_dir)
    })?;
    print_native_attach_info(stage_id, pid);
    focus_window_by_pid(pid)
}

/// Attach to a session directly by session ID
pub fn attach_by_session(session_id: &str, work_dir: &Path) -> Result<()> {
    let session = load_session(work_dir, session_id)?;

    let _backend_type = detect_backend_type(&session).ok_or_else(|| {
        format_manual_mode_error(session_id, session.worktree_path.as_ref(), work_dir)
    })?;

    let pid = session.pid.ok_or_else(|| {
        format_manual_mode_error(session_id, session.worktree_path.as_ref(), work_dir)
    })?;
    let stage_display = session.stage_id.as_deref().unwrap_or(session_id);
    print_native_attach_info(stage_display, pid);
    focus_window_by_pid(pid)
}

/// Print info for native session attachment
///
/// Since native sessions run in separate terminal windows, we focus the window
pub fn print_native_attach_info(stage_name: &str, pid: u32) {
    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│  Focusing native session for: {stage_name:<25}│");
    println!("│  Process ID: {pid:<42}│");
    println!("│                                                         │");
    println!("│  The session runs in a separate terminal window.        │");
    println!("│  Attempting to focus that window...                     │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Print the pre-attach instructions message
///
/// Shows helpful info about detaching and scrolling
pub fn print_attach_instructions(session_name: &str) {
    // Truncate session name if too long to fit in the box
    let display_name = if session_name.len() > 32 {
        format!("{}...", &session_name[..29])
    } else {
        session_name.to_string()
    };

    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│  Attaching to session {display_name:<32}│");
    println!("│                                                         │");
    println!("│  To detach (return to loom): Press Ctrl+B then D        │");
    println!("│  To scroll: Ctrl+B then [ (exit scroll: q)              │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Focus a native terminal window by process ID
///
/// Uses wmctrl or xdotool to focus the window.
/// Returns Ok even if focusing fails (best-effort).
fn focus_window_by_pid(pid: u32) -> Result<()> {
    if let Some(_window_id) = super::try_focus_window_by_pid(pid) {
        println!("Focused window for PID {pid}");
        return Ok(());
    }

    // If we couldn't focus the window, inform the user
    println!("Could not automatically focus the window for PID {pid}.");
    println!("Please manually switch to the terminal window for this session.");
    println!("\nTip: Install wmctrl or xdotool for automatic window focusing:");
    println!("  Ubuntu/Debian: sudo apt-get install wmctrl xdotool");
    println!("  Arch: sudo pacman -S wmctrl xdotool");

    Ok(())
}
