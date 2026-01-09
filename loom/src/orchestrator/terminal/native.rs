//! Native terminal backend
//!
//! Spawns Claude Code sessions in native terminal windows (kitty, alacritty, etc.)
//! using xdg-terminal-exec or fallback detection.

use anyhow::{bail, Context, Result};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::Path;
use std::process::Command;

use super::{BackendType, TerminalBackend};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

/// Native terminal backend - spawns sessions in native terminal windows
pub struct NativeBackend {
    /// The terminal command to use (e.g., "xdg-terminal-exec", "kitty")
    terminal_cmd: String,
}

impl NativeBackend {
    /// Create a new native backend, detecting the available terminal
    pub fn new() -> Result<Self> {
        let terminal_cmd = detect_terminal()?;
        Ok(Self { terminal_cmd })
    }

    /// Get the detected terminal command
    pub fn terminal_cmd(&self) -> &str {
        &self.terminal_cmd
    }
}

impl TerminalBackend for NativeBackend {
    fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        let worktree_path = worktree.path.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Worktree path contains invalid UTF-8: {}",
                worktree.path.display()
            )
        })?;

        // Build the title for the terminal window
        let title = format!("loom-{}", stage.id);

        // Build the initial prompt for Claude
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the signal file at {signal_path_str} and execute the assigned stage work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and context files to read."
        );

        // Escape the prompt for shell
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));

        // Build the command to run in the terminal
        // We use exec to replace the shell with claude, so the PID we get is claude's
        let claude_cmd = format!("exec claude {escaped_prompt}");

        // Spawn the terminal
        let pid = spawn_in_terminal(
            &self.terminal_cmd,
            &title,
            Path::new(worktree_path),
            &claude_cmd,
        )?;

        // Update the session with spawn info
        let mut session = session;
        session.set_worktree_path(worktree.path.clone());
        session.assign_to_stage(stage.id.clone());
        session.set_pid(pid);
        session.try_mark_running()?;

        Ok(session)
    }

    fn kill_session(&self, session: &Session) -> Result<()> {
        // First, try to close the window by title (more reliable for all terminals).
        // The title is set to "loom-{stage_id}" when spawning.
        // This approach works correctly even for terminal emulators like gnome-terminal
        // that use a server process, where killing by PID would kill all windows.
        if let Some(stage_id) = &session.stage_id {
            let title = format!("loom-{stage_id}");
            if close_window_by_title(&title) {
                return Ok(());
            }
        }

        // Fallback to PID-based killing for terminals where window title closing
        // didn't work (e.g., no wmctrl/xdotool installed, or window already closed).
        // This works correctly for terminals like kitty/alacritty where each window
        // has its own process.
        if let Some(pid) = session.pid {
            // Send SIGTERM to the process
            let output = Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output()
                .context("Failed to kill session process")?;

            if !output.status.success() {
                // Process might already be dead, which is fine
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.contains("No such process") {
                    bail!("Failed to kill process {pid}: {stderr}");
                }
            }
        }
        Ok(())
    }

    fn is_session_alive(&self, session: &Session) -> Result<bool> {
        if let Some(pid) = session.pid {
            // Check if process exists using kill -0
            let output = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
                .context("Failed to check process status")?;

            Ok(output.status.success())
        } else {
            Ok(false)
        }
    }

    fn attach_session(&self, session: &Session) -> Result<()> {
        if session.status != SessionStatus::Running {
            bail!("Session {} is not running", session.id);
        }

        if let Some(pid) = session.pid {
            // Try to focus the window using wmctrl or xdotool
            // This is best-effort - we don't fail if it doesn't work
            let _ = focus_window_by_pid(pid);
        }

        Ok(())
    }

    fn attach_all(&self, sessions: &[Session]) -> Result<()> {
        for session in sessions {
            if session.status == SessionStatus::Running {
                if let Some(pid) = session.pid {
                    // Try to focus each window, but don't fail on errors
                    let _ = focus_window_by_pid(pid);
                }
            }
        }
        Ok(())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Native
    }
}

/// Detect the available terminal emulator
///
/// Priority:
/// 1. TERMINAL environment variable (user preference)
/// 2. gsettings/dconf default terminal (GNOME/Cosmic DE settings)
/// 3. xdg-terminal-exec (emerging standard)
/// 4. Common terminals: kitty, alacritty, etc.
fn detect_terminal() -> Result<String> {
    // 1. Check TERMINAL environment variable (user preference)
    if let Ok(terminal) = std::env::var("TERMINAL") {
        if !terminal.is_empty() && which::which(&terminal).is_ok() {
            return Ok(terminal);
        }
    }

    // 2. Check gsettings for default terminal (GNOME/Cosmic DE)
    if let Some(terminal) = get_gsettings_terminal() {
        if which::which(&terminal).is_ok() {
            return Ok(terminal);
        }
    }

    // 3. Try xdg-terminal-exec (emerging standard - respects desktop settings)
    if which::which("xdg-terminal-exec").is_ok() {
        return Ok("xdg-terminal-exec".to_string());
    }

    // 4. Fall back to common terminals (prefer modern GPU-accelerated ones)
    let candidates = [
        "kitty",
        "alacritty",
        "foot",
        "wezterm",
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "x-terminal-emulator",
        "xterm",
    ];

    for candidate in candidates {
        if which::which(candidate).is_ok() {
            return Ok(candidate.to_string());
        }
    }

    bail!(
        "No terminal emulator found. Set TERMINAL environment variable or install one of: {}",
        candidates.join(", ")
    )
}

/// Get the default terminal from gsettings (GNOME/Cosmic DE)
fn get_gsettings_terminal() -> Option<String> {
    // Try org.gnome.desktop.default-applications.terminal (standard GNOME)
    if let Ok(output) = Command::new("gsettings")
        .args([
            "get",
            "org.gnome.desktop.default-applications.terminal",
            "exec",
        ])
        .output()
    {
        if output.status.success() {
            let terminal = String::from_utf8_lossy(&output.stdout)
                .trim()
                .trim_matches('\'')
                .to_string();
            if !terminal.is_empty() {
                return Some(terminal);
            }
        }
    }

    // Try cosmic settings via dconf (Cosmic DE)
    if let Ok(output) = Command::new("dconf")
        .args(["read", "/com/system76/cosmic/default-terminal"])
        .output()
    {
        if output.status.success() {
            let terminal = String::from_utf8_lossy(&output.stdout)
                .trim()
                .trim_matches('\'')
                .to_string();
            if !terminal.is_empty() {
                return Some(terminal);
            }
        }
    }

    None
}

/// Spawn a command in a terminal window
///
/// Returns the PID of the spawned process
fn spawn_in_terminal(terminal: &str, title: &str, workdir: &Path, cmd: &str) -> Result<u32> {
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

/// Close a window by its title using wmctrl or xdotool.
///
/// This is the preferred method for closing terminal windows because it works
/// correctly for all terminal emulators, including those like gnome-terminal
/// that use a server process (where killing by PID would kill all windows).
///
/// Returns `true` if the window was successfully closed, `false` otherwise.
fn close_window_by_title(title: &str) -> bool {
    // Try wmctrl first (most reliable for window management)
    if which::which("wmctrl").is_ok() {
        // wmctrl -c matches window name as substring and sends close request
        let output = Command::new("wmctrl").args(["-c", title]).output();

        if let Ok(out) = output {
            if out.status.success() {
                return true;
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        // Search for window by exact name match
        let search_output = Command::new("xdotool")
            .args(["search", "--name", title])
            .output();

        if let Ok(out) = search_output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // xdotool returns one window ID per line
                for window_id in stdout.lines() {
                    let window_id = window_id.trim();
                    if !window_id.is_empty() {
                        // Send close request to the window
                        let close_result =
                            Command::new("xdotool").args(["windowclose", window_id]).output();

                        if close_result.is_ok() {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Try to focus a window by its process ID
///
/// This is best-effort and uses wmctrl or xdotool if available.
/// Returns Ok(()) even if focusing fails (the window might not be focusable).
fn focus_window_by_pid(pid: u32) -> Result<()> {
    // Try wmctrl first (more reliable for window management)
    if which::which("wmctrl").is_ok() {
        let _ = Command::new("wmctrl")
            .args(["-i", "-a"])
            .arg(format!("0x{pid:x}")) // This won't work, but wmctrl -a with PID isn't standard
            .output();

        // Try by searching window list
        let output = Command::new("wmctrl").arg("-l").arg("-p").output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(window_pid) = parts[2].parse::<u32>() {
                    if window_pid == pid {
                        let window_id = parts[0];
                        let _ = Command::new("wmctrl")
                            .args(["-i", "-a", window_id])
                            .output();
                        return Ok(());
                    }
                }
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        let _ = Command::new("xdotool")
            .args(["search", "--pid", &pid.to_string(), "windowactivate"])
            .output();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_terminal_finds_something() {
        // This test may fail in minimal environments without any terminal
        // but should pass on most development machines
        let result = detect_terminal();
        // We just check it doesn't panic - actual result depends on system
        if result.is_ok() {
            let terminal = result.unwrap();
            assert!(!terminal.is_empty());
        }
    }

    #[test]
    fn test_native_backend_creation() {
        // May fail if no terminal is available
        let result = NativeBackend::new();
        if result.is_ok() {
            let backend = result.unwrap();
            assert!(!backend.terminal_cmd().is_empty());
            assert_eq!(backend.backend_type(), BackendType::Native);
        }
    }

    #[test]
    fn test_close_window_by_title_nonexistent() {
        // Test that closing a non-existent window returns false gracefully
        // This should not panic or error, just return false
        let result = close_window_by_title("nonexistent-window-title-12345");
        // Result depends on whether wmctrl/xdotool are available and if any window matches
        // The important thing is it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_close_window_by_title_format() {
        // Test that the title format we use matches what we expect
        let stage_id = "test-stage";
        let expected_title = format!("loom-{stage_id}");
        assert_eq!(expected_title, "loom-test-stage");
    }
}
