//! Window operations for terminal management
//!
//! Provides functions for closing terminal windows and checking their existence.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{
    close_window_by_title, close_window_by_title_for_terminal, window_exists_by_title,
    window_exists_by_title_for_terminal,
};
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod test_commands;
#[cfg(test)]
mod tests;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::{Command, Output};
#[cfg(all(not(test), any(target_os = "linux", target_os = "macos")))]
use std::time::Duration;

/// Wall-clock bound for a single window-management command.
///
/// These are interactive-speed operations — a window manager or terminal app
/// that has not answered in five seconds is not going to. The bound matters
/// because every one of these calls runs on the orchestrator's single poll
/// thread during session teardown: an unbounded call there stops all
/// orchestration, not just the teardown. See [`crate::process::run_bounded`].
#[cfg(all(not(test), any(target_os = "linux", target_os = "macos")))]
const WINDOW_OP_TIMEOUT: Duration = Duration::from_secs(5);

/// Run a window-management command under [`WINDOW_OP_TIMEOUT`].
///
/// Returns `None` when the command could not be spawned or outlived the
/// deadline. Both are logged: a window that would not close is the difference
/// between a tidy teardown and an orphaned agent process, so it must not pass
/// silently.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn bounded_window_command(command: &mut Command, description: &str) -> Option<Output> {
    #[cfg(test)]
    {
        let _ = description;
        test_commands::run(command)
    }
    #[cfg(not(test))]
    {
        run_window_command(command, description)
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
fn run_window_command(_command: &mut Command, _description: &str) -> Option<Output> {
    panic!("real window commands are forbidden in unit tests");
}

#[cfg(all(not(test), any(target_os = "linux", target_os = "macos")))]
fn run_window_command(command: &mut Command, description: &str) -> Option<Output> {
    match crate::process::run_bounded(command, WINDOW_OP_TIMEOUT) {
        Ok(crate::process::BoundedOutput::Completed(output)) => Some(output),
        Ok(crate::process::BoundedOutput::TimedOut) => {
            tracing::warn!(
                command = %description,
                timeout_secs = WINDOW_OP_TIMEOUT.as_secs(),
                "Window command exceeded its deadline and was killed; \
                 falling back to PID-based teardown"
            );
            None
        }
        Err(e) => {
            tracing::warn!(command = %description, error = %e, "Window command failed to run");
            None
        }
    }
}

/// Close a window by its title using wmctrl or xdotool (Linux).
///
/// This is the preferred method for closing terminal windows because it works
/// correctly for all terminal emulators, including those like gnome-terminal
/// that use a server process (where killing by PID would kill all windows).
///
/// Returns `true` if the window was successfully closed, `false` otherwise.
#[cfg(target_os = "linux")]
pub fn close_window_by_title(title: &str) -> bool {
    // Try wmctrl first (most reliable for window management)
    if which::which("wmctrl").is_ok() {
        // `-F` makes `-c` match the FULL window name exactly (not a substring),
        // so closing `loom-auth` never closes `loom-auth-tests`.
        let mut command = Command::new("wmctrl");
        command.args(["-F", "-c", title]);

        if let Some(out) = bounded_window_command(&mut command, "wmctrl -c") {
            if out.status.success() {
                return true;
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        // xdotool `search --name` treats the argument as a regex; anchor it so
        // only the exact title matches (`^loom-auth$` won't match
        // `loom-auth-tests`). The title is regex-escaped first.
        let anchored = format!("^{}$", regex_escape(title));
        let mut search = Command::new("xdotool");
        search.args(["search", "--name", &anchored]);

        if let Some(out) = bounded_window_command(&mut search, "xdotool search") {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // xdotool returns one window ID per line
                for window_id in stdout.lines() {
                    let window_id = window_id.trim();
                    if !window_id.is_empty() {
                        // Send close request to the window
                        let mut close = Command::new("xdotool");
                        close.args(["windowclose", window_id]);

                        if bounded_window_command(&mut close, "xdotool windowclose").is_some() {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Escape a literal string for use inside an extended regular expression.
///
/// xdotool's `--name` matches with a regex; window titles are
/// `loom-<stage-id>` where the id allows `[a-zA-Z0-9_-]`. `-` and `_` are not
/// regex metacharacters, but we escape the full POSIX-ERE metacharacter set so
/// anchoring (`^…$`) is robust against any title.
#[cfg(target_os = "linux")]
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Check if a window with the given title exists (Linux)
///
/// Uses wmctrl or xdotool to check if a window with the given title exists.
/// Returns true if found, false otherwise (or if tools unavailable).
#[cfg(target_os = "linux")]
pub fn window_exists_by_title(title: &str) -> bool {
    // Try wmctrl first (most reliable for window management)
    if which::which("wmctrl").is_ok() {
        // `wmctrl -l` lists windows as: <win-id> <desktop> <host> <title...>.
        // The title is everything after the first three whitespace-separated
        // columns; compare it EXACTLY so `loom-auth` doesn't match
        // `loom-auth-tests`.
        let mut command = Command::new("wmctrl");
        command.arg("-l");

        if let Some(out) = bounded_window_command(&mut command, "wmctrl -l") {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if let Some(window_title) = wmctrl_list_title(line) {
                        if window_title == title {
                            return true;
                        }
                    }
                }
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        // Anchor the regex so only the exact title matches.
        let anchored = format!("^{}$", regex_escape(title));
        let mut search = Command::new("xdotool");
        search.args(["search", "--name", &anchored]);

        if let Some(out) = bounded_window_command(&mut search, "xdotool search") {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // xdotool returns one window ID per line
                // If any window IDs are returned, the window exists
                for window_id in stdout.lines() {
                    let window_id = window_id.trim();
                    if !window_id.is_empty() {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Extract the title column from a `wmctrl -l` line.
///
/// Format: `<window-id> <desktop> <client-host> <title (may contain spaces)>`.
/// Returns the title with surrounding whitespace trimmed, or `None` if the line
/// has fewer than the four expected columns.
#[cfg(target_os = "linux")]
fn wmctrl_list_title(line: &str) -> Option<&str> {
    // Skip three columns (id, desktop, host), keep the remainder as the title.
    let mut rest = line.trim_start();
    for _ in 0..3 {
        let space = rest.find(char::is_whitespace)?;
        rest = rest[space..].trim_start();
    }
    Some(rest.trim_end())
}
