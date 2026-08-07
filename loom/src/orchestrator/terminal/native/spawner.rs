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
// Note: macOS terminals via AppleScript need more time to:
// 1. Launch the terminal app
// 2. Open a window
// 3. Start the shell
// 4. Execute the wrapper script
// 5. Write the PID file
const CLAUDE_STARTUP_DELAY_MS: u64 = 1000; // Increased from 500ms
const PID_DISCOVERY_TIMEOUT_SECS: u64 = 5; // Increased from 3s
const PID_FILE_RETRY_DELAY_MS: u64 = 200;
const PID_FILE_MAX_RETRIES: u32 = 10; // Total 2s of retries

/// Spawn a background thread to reap a child process when it exits.
///
/// This prevents zombie processes by ensuring `wait()` is called on the child.
/// The thread runs until the child exits, then terminates.
fn spawn_reaper_thread(mut child: std::process::Child) {
    thread::spawn(move || {
        // Wait for the child process to exit - this reaps the zombie
        let _ = child.wait();
    });
}

/// Spawn a command in a terminal window
///
/// # Arguments
/// * `terminal` - The terminal emulator to use
/// * `title` - Window title for the terminal
/// * `workdir` - Working directory for the command
/// * `cmd` - The command to execute
/// * `work_dir` - Optional .work directory for PID tracking
/// * `pid_key` - Optional per-session PID-file key (tracking_key + session.id)
/// * `session_id` - Optional LOOM_SESSION_ID marker for `/proc`-based discovery
///
/// # Returns
/// The PID of the spawned process. If `work_dir`, `pid_key`, and `session_id`
/// are provided, attempts to resolve the actual Claude PID instead of the
/// terminal PID.
pub fn spawn_in_terminal(
    terminal: &TerminalEmulator,
    title: &str,
    workdir: &Path,
    cmd: &str,
    work_dir: Option<&Path>,
    pid_key: Option<&str>,
    session_id: Option<&str>,
) -> Result<u32> {
    let mut command = terminal.build_command(title, workdir, cmd);

    let child = command.spawn().with_context(|| {
        format!(
            "Failed to spawn terminal '{}'. Is it installed?",
            terminal.binary()
        )
    })?;

    let terminal_pid = child.id();

    // Spawn a reaper thread to prevent zombie processes.
    // When the terminal process exits, the thread will call wait() to reap it.
    spawn_reaper_thread(child);

    // If PID tracking is enabled, try to resolve the actual Claude PID,
    // falling back to the terminal PID rather than failing completely.
    if let (Some(work_dir), Some(pid_key), Some(session_id)) = (work_dir, pid_key, session_id) {
        return await_session_pid(work_dir, pid_key, workdir, session_id, Some(terminal_pid));
    }

    Ok(terminal_pid)
}

/// Wait for the process a wrapper script `exec`s into to become discoverable,
/// and return its PID.
///
/// Shared by the native and tmux lanes: both spawn a wrapper script and then
/// need the same layered wait — PID-file poll, then `/proc`-constrained
/// discovery — to find the process it `exec`'d into.
///
/// # Arguments
/// * `work_dir` - The `.work` directory for PID-file tracking
/// * `pid_key` - The per-session PID-file key (tracking_key + session.id)
/// * `workdir` - The spawned process's working directory (used to constrain
///   `/proc` discovery to this session)
/// * `session_id` - The `LOOM_SESSION_ID` marker the wrapper script exports
/// * `fallback_pid` - Returned if no PID is discovered by either layer.
///   `None` when there is no meaningful fallback (e.g. the tmux lane, where
///   there is no terminal PID to fall back to).
///
/// # Returns
/// The first live PID found by the layers above, else `fallback_pid` if
/// `Some`, else `Err` naming `pid_key`.
pub(crate) fn await_session_pid(
    work_dir: &Path,
    pid_key: &str,
    workdir: &Path,
    session_id: &str,
    fallback_pid: Option<u32>,
) -> Result<u32> {
    // Wait initial delay for the process to start.
    thread::sleep(Duration::from_millis(CLAUDE_STARTUP_DELAY_MS));

    // Try to read the PID from the PID file with retries.
    // The wrapper script may take time to execute and write the PID.
    for retry in 0..PID_FILE_MAX_RETRIES {
        if let Some(claude_pid) = pid_tracking::read_pid_file(work_dir, pid_key) {
            // Verify the PID is actually alive
            if crate::process::is_process_alive(claude_pid) {
                return Ok(claude_pid);
            }
        }

        if retry < PID_FILE_MAX_RETRIES - 1 {
            thread::sleep(Duration::from_millis(PID_FILE_RETRY_DELAY_MS));
        }
    }

    // Fallback: discover the process constrained by this session's
    // LOOM_SESSION_ID marker, so we never latch onto a user's interactive
    // claude that shares the spawn directory (e.g. the repo root).
    if let Some(claude_pid) = pid_tracking::discover_claude_pid(
        workdir,
        session_id,
        Duration::from_secs(PID_DISCOVERY_TIMEOUT_SECS),
    ) {
        return Ok(claude_pid);
    }

    fallback_pid.ok_or_else(|| anyhow::anyhow!("No PID discovered for session (pid_key={pid_key})"))
}
