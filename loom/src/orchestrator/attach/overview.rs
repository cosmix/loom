//! Multi-session overview functionality.
//!
//! Functions to manage multi-session views for native backends.
//! For native: focuses all terminal windows sequentially.

use anyhow::{bail, Result};

use super::{try_focus_window_by_pid, AttachableSession, SessionBackend};

/// Print warning when there are many sessions
pub fn print_many_sessions_warning(count: usize) {
    if count > 6 {
        eprintln!("\nWarning: {count} sessions to focus.");
        eprintln!("Consider using 'loom attach all --gui' for better window management.\n");
    }
}

/// Create overview session (not supported with native backend)
pub fn create_overview_session(
    _sessions: &[AttachableSession],
    _detach_existing: bool,
) -> Result<String> {
    bail!("Overview sessions are not supported with native backend. Use 'loom attach all --gui' instead.")
}

/// Print overview instructions (not supported with native backend)
pub fn print_overview_instructions(_session_count: usize) {
    eprintln!("Overview sessions are not supported with native backend.");
    eprintln!("Use 'loom attach all' or 'loom attach all --gui' instead.");
}

/// Attach to overview session (not supported with native backend)
pub fn attach_overview_session(_overview_name: &str) -> Result<()> {
    bail!("Overview sessions are not supported with native backend. Use 'loom attach all --gui' instead.")
}

/// Create tiled overview (not supported with native backend)
pub fn create_tiled_overview(
    _sessions: &[AttachableSession],
    _layout: &str,
    _detach_existing: bool,
) -> Result<String> {
    bail!("Tiled overview is not supported with native backend. Use 'loom attach all --gui' instead.")
}

/// Print tiled overview instructions (not supported with native backend)
pub fn print_tiled_instructions(_session_count: usize) {
    eprintln!("Tiled overview is not supported with native backend.");
    eprintln!("Use 'loom attach all' or 'loom attach all --gui' instead.");
}

/// Attach to all native sessions by focusing their windows
///
/// This is the native backend equivalent of overview.
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
        let SessionBackend::Native { pid } = &session.backend;
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
