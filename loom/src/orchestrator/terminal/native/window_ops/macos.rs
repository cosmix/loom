//! AppleScript window operations for macOS terminals.

use super::bounded_window_command;
use crate::orchestrator::terminal::emulator::{escape_applescript_string, TerminalEmulator};
use std::process::Command;

/// Helper function to execute AppleScript and return a boolean result.
///
/// Bounded by [`super::WINDOW_OP_TIMEOUT`]. `osascript` sends an Apple Event and will
/// wait forever on the receiving app: a first-run TCC Automation prompt (which
/// a detached daemon cannot surface), a "close window with running processes?"
/// modal, or a wedged terminal all park the call indefinitely. Unbounded, that
/// freezes the orchestrator's poll loop.
fn execute_applescript_bool(script: &str) -> bool {
    let mut command = Command::new("osascript");
    command.arg("-e").arg(script);

    bounded_window_command(&mut command, "osascript")
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true")
        .unwrap_or(false)
}

/// Close a Terminal.app window by title (macOS)
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
pub fn close_window_by_title(title: &str) -> bool {
    let preferred = std::env::var("LOOM_TERMINAL")
        .ok()
        .and_then(|name| TerminalEmulator::from_name(&name));
    close_window_by_title_with_preference(title, preferred)
}

pub(super) fn close_window_by_title_with_preference(
    title: &str,
    preferred: Option<TerminalEmulator>,
) -> bool {
    if let Some(terminal) = preferred {
        if close_window_by_title_for_terminal(title, &terminal) {
            return true;
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

/// Check if a Terminal.app window exists by title (macOS)
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
pub fn window_exists_by_title(title: &str) -> bool {
    let preferred = std::env::var("LOOM_TERMINAL")
        .ok()
        .and_then(|name| TerminalEmulator::from_name(&name));
    window_exists_by_title_with_preference(title, preferred)
}

pub(super) fn window_exists_by_title_with_preference(
    title: &str,
    preferred: Option<TerminalEmulator>,
) -> bool {
    if let Some(terminal) = preferred {
        if window_exists_by_title_for_terminal(title, &terminal) {
            return true;
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
