//! GUI terminal emulator support for multi-session attachment.
//!
//! Provides functionality to spawn separate GUI terminal windows for
//! each loom session, using the detected terminal emulator.
//! Supports native sessions.

use anyhow::{anyhow, Result};

use super::{AttachableSession, SessionBackend};
use crate::orchestrator::terminal::native::{detect_terminal, focus_window_by_pid};

// Re-export TerminalEmulator from the terminal module
pub use crate::orchestrator::terminal::TerminalEmulator;

/// Spawn GUI terminal windows for each session
///
/// For native sessions: focuses existing windows instead of spawning new ones.
pub fn spawn_gui_windows(sessions: &[AttachableSession], _detach_existing: bool) -> Result<()> {
    let _terminal = detect_terminal().map_err(|e| {
        anyhow!(
            "No supported terminal emulator found: {e}\n\
             Supported: gnome-terminal, konsole, xfce4-terminal, mate-terminal, \
             alacritty, kitty, wezterm, xterm, urxvt"
        )
    })?;

    // Only native sessions are supported
    let native_sessions: Vec<_> = sessions.iter().filter(|s| s.is_native()).collect();

    if !native_sessions.is_empty() {
        println!(
            "\nFocusing {} native session(s)...\n",
            native_sessions.len()
        );

        for session in &native_sessions {
            let stage_display = session
                .stage_name
                .as_ref()
                .or(session.stage_id.as_ref())
                .map(|s| s.as_str())
                .unwrap_or(&session.session_id);

            let SessionBackend::Native { pid } = &session.backend;
            if focus_window_by_pid(*pid).is_ok() {
                println!("  Focused: {stage_display} (PID: {pid})");
            } else {
                eprintln!("  Could not focus: {stage_display} (PID: {pid})");
            }
        }
    }

    let total = native_sessions.len();
    println!("\nProcessed {total} session(s).");

    Ok(())
}
