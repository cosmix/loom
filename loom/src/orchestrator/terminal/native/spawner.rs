//! Terminal spawning logic
//!
//! Handles spawning commands in various terminal emulators.

use anyhow::{Context, Result};
use std::path::Path;
use std::thread;
use std::time::Duration;

use super::super::emulator::TerminalEmulator;
use super::pid_tracking;

// Terminal spawning timing constants
const CLAUDE_STARTUP_DELAY_MS: u64 = 500;
const PID_DISCOVERY_TIMEOUT_SECS: u64 = 3;

/// Spawn a command in a terminal window
///
/// # Arguments
/// * `terminal` - The terminal emulator to use
/// * `title` - Window title for the terminal
/// * `workdir` - Working directory for the command
/// * `cmd` - The command to execute
/// * `work_dir` - Optional .work directory for PID tracking
/// * `stage_id` - Optional stage ID for PID file
///
/// # Returns
/// The PID of the spawned process. If `work_dir` and `stage_id` are provided,
/// attempts to resolve the actual Claude PID instead of the terminal PID.
pub fn spawn_in_terminal(
    terminal: &TerminalEmulator,
    title: &str,
    workdir: &Path,
    cmd: &str,
    work_dir: Option<&Path>,
    stage_id: Option<&str>,
) -> Result<u32> {
    let mut command = terminal.build_command(title, workdir, cmd);

    let child = command
        .spawn()
        .with_context(|| format!("Failed to spawn terminal '{}'. Is it installed?", terminal.binary()))?;

    let terminal_pid = child.id();

    // If PID tracking is enabled, try to resolve the actual Claude PID
    if let (Some(work_dir), Some(stage_id)) = (work_dir, stage_id) {
        // Wait for the PID file to be created by the wrapper script
        thread::sleep(Duration::from_millis(CLAUDE_STARTUP_DELAY_MS));

        // Try to read the PID from the PID file
        if let Some(claude_pid) = pid_tracking::read_pid_file(work_dir, stage_id) {
            // Verify the PID is actually alive
            if pid_tracking::check_pid_alive(claude_pid) {
                return Ok(claude_pid);
            }
        }

        // Fallback: try to discover the Claude process by scanning /proc
        if let Some(claude_pid) = pid_tracking::discover_claude_pid(workdir, Duration::from_secs(PID_DISCOVERY_TIMEOUT_SECS))
        {
            return Ok(claude_pid);
        }

        // If we couldn't find the Claude PID, fall back to the terminal PID
        // (better than failing completely)
    }

    Ok(terminal_pid)
}
