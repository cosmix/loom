//! Window operations for terminal management
//!
//! Provides functions for closing and focusing terminal windows.

#[cfg(target_os = "macos")]
use crate::orchestrator::terminal::emulator::{escape_applescript_string, TerminalEmulator};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;

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
        let output = Command::new("wmctrl").args(["-F", "-c", title]).output();

        if let Ok(out) = output {
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
        let search_output = Command::new("xdotool")
            .args(["search", "--name", &anchored])
            .output();

        if let Ok(out) = search_output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // xdotool returns one window ID per line
                for window_id in stdout.lines() {
                    let window_id = window_id.trim();
                    if !window_id.is_empty() {
                        // Send close request to the window
                        let close_result = Command::new("xdotool")
                            .args(["windowclose", window_id])
                            .output();

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

/// Helper function to execute AppleScript and return a boolean result
#[cfg(target_os = "macos")]
fn execute_applescript_bool(script: &str) -> bool {
    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map(|out| {
            if out.status.success() {
                String::from_utf8_lossy(&out.stdout).trim() == "true"
            } else {
                false
            }
        })
        .unwrap_or(false)
}

/// Close a Terminal.app window by title (macOS)
#[cfg(target_os = "macos")]
fn close_terminal_app_window(escaped_title: &str) -> bool {
    let script = format!(
        r#"tell application "Terminal"
    if it is running then
        set windowList to every window whose name is "{}"
        if (count of windowList) > 0 then
            repeat with w in windowList
                close w
            end repeat
            return true
        end if
    end if
    return false
end tell"#,
        escaped_title
    );

    execute_applescript_bool(&script)
}

/// Close an iTerm2 window by title (macOS)
#[cfg(target_os = "macos")]
fn close_iterm2_window(escaped_title: &str) -> bool {
    let script = format!(
        r#"tell application "iTerm2"
    if it is running then
        set windowList to every window whose name is "{}"
        if (count of windowList) > 0 then
            repeat with w in windowList
                close w
            end repeat
            return true
        end if
    end if
    return false
end tell"#,
        escaped_title
    );

    execute_applescript_bool(&script)
}

/// Close a cross-platform terminal window by title (macOS)
///
/// For terminals like Ghostty, Kitty, Alacritty, and Wezterm that run on multiple platforms,
/// use macOS's standard window closing API via AppleScript.
#[cfg(target_os = "macos")]
fn close_cross_platform_terminal_window(escaped_title: &str, terminal: &TerminalEmulator) -> bool {
    let app_name = match terminal {
        TerminalEmulator::Ghostty => "Ghostty",
        TerminalEmulator::Kitty => "kitty",
        TerminalEmulator::Alacritty => "Alacritty",
        TerminalEmulator::Wezterm => "WezTerm",
        _ => return false,
    };

    let script = format!(
        r#"tell application "{}"
    if it is running then
        set windowList to every window whose name is "{}"
        if (count of windowList) > 0 then
            repeat with w in windowList
                close w
            end repeat
            return true
        end if
    end if
    return false
end tell"#,
        app_name, escaped_title
    );

    execute_applescript_bool(&script)
}

/// Close a window by title for a specific terminal emulator (macOS).
///
/// This is the preferred method when you know which terminal is being used.
/// Returns `true` if the window was successfully closed, `false` otherwise.
#[cfg(target_os = "macos")]
pub fn close_window_by_title_for_terminal(title: &str, terminal: &TerminalEmulator) -> bool {
    let escaped_title = escape_applescript_string(title);

    match terminal {
        TerminalEmulator::TerminalApp => close_terminal_app_window(&escaped_title),
        TerminalEmulator::ITerm2 => close_iterm2_window(&escaped_title),
        TerminalEmulator::Ghostty
        | TerminalEmulator::Kitty
        | TerminalEmulator::Alacritty
        | TerminalEmulator::Wezterm => {
            close_cross_platform_terminal_window(&escaped_title, terminal)
        }
        _ => false,
    }
}

/// Close a window by its title using AppleScript (macOS).
///
/// This function tries to determine the terminal from LOOM_TERMINAL env var,
/// then falls back to trying all known terminals.
///
/// Returns `true` if the window was successfully closed, `false` otherwise.
#[cfg(target_os = "macos")]
pub fn close_window_by_title(title: &str) -> bool {
    // Try to determine which terminal to use from LOOM_TERMINAL env var
    if let Ok(terminal_name) = std::env::var("LOOM_TERMINAL") {
        if let Some(terminal) = TerminalEmulator::from_name(&terminal_name) {
            if close_window_by_title_for_terminal(title, &terminal) {
                return true;
            }
        }
    }

    // Fallback: try all known terminals
    let escaped_title = escape_applescript_string(title);

    // Try Terminal.app
    if close_terminal_app_window(&escaped_title) {
        return true;
    }

    // Try iTerm2
    if close_iterm2_window(&escaped_title) {
        return true;
    }

    // Try Ghostty
    if close_cross_platform_terminal_window(&escaped_title, &TerminalEmulator::Ghostty) {
        return true;
    }

    // Try Kitty
    if close_cross_platform_terminal_window(&escaped_title, &TerminalEmulator::Kitty) {
        return true;
    }

    // Try Alacritty
    if close_cross_platform_terminal_window(&escaped_title, &TerminalEmulator::Alacritty) {
        return true;
    }

    // Try Wezterm
    if close_cross_platform_terminal_window(&escaped_title, &TerminalEmulator::Wezterm) {
        return true;
    }

    false
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
        let output = Command::new("wmctrl").arg("-l").output();

        if let Ok(out) = output {
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
        let search_output = Command::new("xdotool")
            .args(["search", "--name", &anchored])
            .output();

        if let Ok(out) = search_output {
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

/// Check if a Terminal.app window exists by title (macOS)
#[cfg(target_os = "macos")]
fn terminal_app_window_exists(escaped_title: &str) -> bool {
    let script = format!(
        r#"tell application "Terminal"
    set windowList to every window whose name is "{}"
    return (count of windowList) > 0
end tell"#,
        escaped_title
    );

    execute_applescript_bool(&script)
}

/// Check if an iTerm2 window exists by title (macOS)
#[cfg(target_os = "macos")]
fn iterm2_window_exists(escaped_title: &str) -> bool {
    let script = format!(
        r#"tell application "iTerm2"
    set windowList to every window whose name is "{}"
    return (count of windowList) > 0
end tell"#,
        escaped_title
    );

    execute_applescript_bool(&script)
}

/// Check if a cross-platform terminal window exists by title (macOS)
#[cfg(target_os = "macos")]
fn cross_platform_terminal_window_exists(escaped_title: &str, terminal: &TerminalEmulator) -> bool {
    let app_name = match terminal {
        TerminalEmulator::Ghostty => "Ghostty",
        TerminalEmulator::Kitty => "kitty",
        TerminalEmulator::Alacritty => "Alacritty",
        TerminalEmulator::Wezterm => "WezTerm",
        _ => return false,
    };

    let script = format!(
        r#"tell application "{}"
    set windowList to every window whose name is "{}"
    return (count of windowList) > 0
end tell"#,
        app_name, escaped_title
    );

    execute_applescript_bool(&script)
}

/// Check if a window with the given title exists for a specific terminal (macOS)
///
/// Uses AppleScript to check if a window with the given title exists.
/// Returns true if found, false otherwise.
#[cfg(target_os = "macos")]
pub fn window_exists_by_title_for_terminal(title: &str, terminal: &TerminalEmulator) -> bool {
    let escaped_title = escape_applescript_string(title);

    match terminal {
        TerminalEmulator::TerminalApp => terminal_app_window_exists(&escaped_title),
        TerminalEmulator::ITerm2 => iterm2_window_exists(&escaped_title),
        TerminalEmulator::Ghostty
        | TerminalEmulator::Kitty
        | TerminalEmulator::Alacritty
        | TerminalEmulator::Wezterm => {
            cross_platform_terminal_window_exists(&escaped_title, terminal)
        }
        _ => false,
    }
}

/// Check if a window with the given title exists (macOS)
///
/// Uses AppleScript to check if a window with the given title exists.
/// Tries LOOM_TERMINAL env var first, then falls back to trying all known terminals.
/// Returns true if found, false otherwise.
#[cfg(target_os = "macos")]
pub fn window_exists_by_title(title: &str) -> bool {
    // Try to determine which terminal to use from LOOM_TERMINAL env var
    if let Ok(terminal_name) = std::env::var("LOOM_TERMINAL") {
        if let Some(terminal) = TerminalEmulator::from_name(&terminal_name) {
            if window_exists_by_title_for_terminal(title, &terminal) {
                return true;
            }
        }
    }

    // Fallback: try all known terminals
    let escaped_title = escape_applescript_string(title);

    // Check Terminal.app
    if terminal_app_window_exists(&escaped_title) {
        return true;
    }

    // Check iTerm2
    if iterm2_window_exists(&escaped_title) {
        return true;
    }

    // Check Ghostty
    if cross_platform_terminal_window_exists(&escaped_title, &TerminalEmulator::Ghostty) {
        return true;
    }

    // Check Kitty
    if cross_platform_terminal_window_exists(&escaped_title, &TerminalEmulator::Kitty) {
        return true;
    }

    // Check Alacritty
    if cross_platform_terminal_window_exists(&escaped_title, &TerminalEmulator::Alacritty) {
        return true;
    }

    // Check Wezterm
    if cross_platform_terminal_window_exists(&escaped_title, &TerminalEmulator::Wezterm) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_window_exists_by_title_nonexistent() {
        // Test that checking for a non-existent window returns false gracefully
        // This should not panic or error, just return false
        let result = window_exists_by_title("nonexistent-window-title-98765");
        // If wmctrl/xdotool are not available, this will return false
        // If they are available, it should also return false for this non-existent window
        assert!(!result, "Non-existent window should not be found");
    }

    #[test]
    fn test_window_exists_by_title_format() {
        // Test that we can construct valid title strings for the function
        let stage_id = "check-stage";
        let title = format!("loom-{stage_id}");
        // This tests the function doesn't panic with valid input
        let _ = window_exists_by_title(&title);
        // The actual result depends on whether such a window exists
    }

    // O-5: prefix-sharing stage IDs must NOT collide. The substring matching
    // these helpers replaced would have matched `loom-auth` against
    // `loom-auth-tests`, killing a healthy sibling / suppressing crash
    // detection. The exact-match logic below proves the two never collide.

    #[cfg(target_os = "linux")]
    #[test]
    fn test_wmctrl_list_title_exact_no_prefix_collision() {
        // wmctrl -l line: <id> <desktop> <host> <title>
        let auth = "0x01 0 host loom-auth";
        let auth_tests = "0x02 0 host loom-auth-tests";

        assert_eq!(wmctrl_list_title(auth), Some("loom-auth"));
        assert_eq!(wmctrl_list_title(auth_tests), Some("loom-auth-tests"));

        // Exact comparison: searching for `loom-auth` must NOT match
        // `loom-auth-tests` (the old substring `.contains` bug).
        assert_ne!(wmctrl_list_title(auth_tests), Some("loom-auth"));
        assert_eq!(wmctrl_list_title(auth), Some("loom-auth"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_wmctrl_list_title_handles_spaces_and_short_lines() {
        // Titles may contain spaces; everything after column 3 is the title.
        assert_eq!(
            wmctrl_list_title("0x03 0 host loom-auth — bash"),
            Some("loom-auth — bash")
        );
        // Fewer than 4 columns → None (no title field present).
        assert_eq!(wmctrl_list_title("0x04 0 host"), None);
        assert_eq!(wmctrl_list_title("0x05 0"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_regex_escape_anchors_prevent_prefix_match() {
        // The anchored regex for `loom-auth` is `^loom-auth$`; it must not
        // be a prefix of the anchored regex for `loom-auth-tests`.
        let auth = format!("^{}$", regex_escape("loom-auth"));
        let auth_tests = format!("^{}$", regex_escape("loom-auth-tests"));
        assert_eq!(auth, "^loom-auth$");
        assert_eq!(auth_tests, "^loom-auth-tests$");
        assert_ne!(auth, auth_tests);

        // Metacharacters in a title are escaped so anchoring stays literal.
        assert_eq!(regex_escape("loom-a.b+c"), "loom-a\\.b\\+c");
    }
}
