//! Terminal spawning logic
//!
//! Handles spawning commands in various terminal emulators.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Spawn a command in a terminal window
///
/// Returns the PID of the spawned process
pub fn spawn_in_terminal(terminal: &str, title: &str, workdir: &Path, cmd: &str) -> Result<u32> {
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

    Ok(child.id())
}
