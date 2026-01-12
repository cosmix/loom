//! Terminal spawning logic
//!
//! Handles spawning commands in various terminal emulators.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use super::pid_tracking;

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
    terminal: &str,
    title: &str,
    workdir: &Path,
    cmd: &str,
    work_dir: Option<&Path>,
    stage_id: Option<&str>,
) -> Result<u32> {
    let mut command = Command::new(terminal);

    // Configure based on terminal type
    match terminal {
        "xdg-terminal-exec" => {
            command
                .arg(format!("--title={title}"))
                .arg(format!("--dir={}", workdir.display()))
                .arg("--")
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        "kitty" => {
            command
                .arg("--title")
                .arg(title)
                .arg("--directory")
                .arg(workdir)
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        "alacritty" => {
            command
                .arg("--title")
                .arg(title)
                .arg("--working-directory")
                .arg(workdir)
                .arg("-e")
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        "foot" => {
            command
                .arg("--title")
                .arg(title)
                .arg("--working-directory")
                .arg(workdir)
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        "wezterm" => {
            command
                .arg("start")
                .arg("--cwd")
                .arg(workdir)
                .arg("--")
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        "gnome-terminal" => {
            command
                .arg("--title")
                .arg(title)
                .arg("--working-directory")
                .arg(workdir)
                .arg("--")
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        "konsole" => {
            command
                .arg("--workdir")
                .arg(workdir)
                .arg("-e")
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        "xfce4-terminal" => {
            command
                .arg("--title")
                .arg(title)
                .arg("--working-directory")
                .arg(workdir)
                .arg("-x")
                .arg("bash")
                .arg("-c")
                .arg(cmd);
        }
        _ => {
            // Generic fallback - most terminals support -e
            command.arg("-e").arg("bash").arg("-c").arg(format!(
                "cd {} && {}",
                workdir.display(),
                cmd
            ));
        }
    }

    let child = command
        .spawn()
        .with_context(|| format!("Failed to spawn terminal '{terminal}'. Is it installed?"))?;

    let terminal_pid = child.id();

    // If PID tracking is enabled, try to resolve the actual Claude PID
    if let (Some(work_dir), Some(stage_id)) = (work_dir, stage_id) {
        // Wait for the PID file to be created by the wrapper script
        thread::sleep(Duration::from_millis(500));

        // Try to read the PID from the PID file
        if let Some(claude_pid) = pid_tracking::read_pid_file(work_dir, stage_id) {
            // Verify the PID is actually alive
            if pid_tracking::check_pid_alive(claude_pid) {
                return Ok(claude_pid);
            }
        }

        // Fallback: try to discover the Claude process by scanning /proc
        if let Some(claude_pid) = pid_tracking::discover_claude_pid(workdir, Duration::from_secs(3))
        {
            return Ok(claude_pid);
        }

        // If we couldn't find the Claude PID, fall back to the terminal PID
        // (better than failing completely)
    }

    Ok(terminal_pid)
}
