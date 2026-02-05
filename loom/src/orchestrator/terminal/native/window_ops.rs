//! Window operations for terminal management
//!
//! Provides functions for closing and focusing terminal windows.

use crate::orchestrator::terminal::emulator::TerminalEmulator;
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
        set windowList to every window whose name contains "{}"
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
        set windowList to every window whose name contains "{}"
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
        set windowList to every window whose name contains "{}"
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
    let escaped_title = title.replace('"', "\\\"");

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
    let escaped_title = title.replace('"', "\\\"");

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
        // wmctrl -l lists all windows
        let output = Command::new("wmctrl").arg("-l").output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // Check if any line contains the title
                for line in stdout.lines() {
                    if line.contains(title) {
                        return true;
                    }
                }
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        // Search for window by name match
        let search_output = Command::new("xdotool")
            .args(["search", "--name", title])
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

/// Check if a Terminal.app window exists by title (macOS)
#[cfg(target_os = "macos")]
fn terminal_app_window_exists(escaped_title: &str) -> bool {
    let script = format!(
        r#"tell application "Terminal"
    set windowList to every window whose name contains "{}"
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
    set windowList to every window whose name contains "{}"
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
    set windowList to every window whose name contains "{}"
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
    let escaped_title = title.replace('"', "\\\"");

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
    let escaped_title = title.replace('"', "\\\"");

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
}
