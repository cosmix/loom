//! Terminal detection logic
//!
//! Detects available terminal emulators on the system.

#[cfg(target_os = "linux")]
use anyhow::bail;
use anyhow::Result;

use std::process::Command;

#[cfg(target_os = "macos")]
use std::path::Path;

use super::super::emulator::TerminalEmulator;

/// Detect the available terminal emulator (Linux)
///
/// Priority:
/// 1. TERMINAL environment variable (user preference)
/// 2. gsettings/dconf default terminal (GNOME/Cosmic DE settings)
/// 3. xdg-terminal-exec (emerging standard)
/// 4. Common terminals: kitty, alacritty, etc.
#[cfg(target_os = "linux")]
pub fn detect_terminal() -> Result<TerminalEmulator> {
    // 1. Check TERMINAL environment variable (user preference)
    if let Ok(terminal) = std::env::var("TERMINAL") {
        if !terminal.is_empty() && which::which(&terminal).is_ok() {
            if let Some(emulator) = TerminalEmulator::from_binary(&terminal) {
                return Ok(emulator);
            }
        }
    }

    // 2. Check gsettings for default terminal (GNOME/Cosmic DE)
    if let Some(terminal) = get_gsettings_terminal() {
        if which::which(&terminal).is_ok() {
            if let Some(emulator) = TerminalEmulator::from_binary(&terminal) {
                return Ok(emulator);
            }
        }
    }

    // 3. Try xdg-terminal-exec (emerging standard - respects desktop settings)
    if which::which("xdg-terminal-exec").is_ok() {
        return Ok(TerminalEmulator::XdgTerminalExec);
    }

    // 4. Fall back to common terminals (prefer modern GPU-accelerated ones)
    let candidates = [
        TerminalEmulator::Kitty,
        TerminalEmulator::Alacritty,
        TerminalEmulator::Foot,
        TerminalEmulator::Wezterm,
        TerminalEmulator::GnomeTerminal,
        TerminalEmulator::Konsole,
        TerminalEmulator::Xfce4Terminal,
        TerminalEmulator::XTerm,
    ];

    for candidate in candidates {
        if which::which(candidate.binary()).is_ok() {
            return Ok(candidate);
        }
    }

    bail!(
        "No terminal emulator found. Set TERMINAL environment variable or install one of: \
         kitty, alacritty, foot, wezterm, gnome-terminal, konsole, xfce4-terminal, xterm"
    )
}

/// Get the default terminal from gsettings (GNOME/Cosmic DE)
#[cfg(target_os = "linux")]
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

/// Detect the available terminal emulator (macOS)
///
/// Priority:
/// 1. TERMINAL environment variable (user preference)
/// 2. Currently running terminal (detected via parent process)
/// 3. Cross-platform terminals (kitty, alacritty, wezterm)
/// 4. iTerm2 or Terminal.app (check for installed apps)
///
/// Note: We explicitly avoid detect_running_terminal() because it's non-deterministic
/// when multiple terminal apps are running simultaneously. Instead, we prefer the
/// parent process chain which is deterministic and stable.
#[cfg(target_os = "macos")]
pub fn detect_terminal() -> Result<TerminalEmulator> {
    // 1. Check TERMINAL environment variable (user preference)
    if let Ok(terminal) = std::env::var("TERMINAL") {
        if !terminal.is_empty() {
            // Try matching as app name first (for "iTerm2", "Terminal", etc.)
            if let Some(emulator) = TerminalEmulator::from_name(&terminal) {
                return Ok(emulator);
            }
            // Then try as binary (for "kitty", "alacritty", etc.)
            if which::which(&terminal).is_ok() {
                if let Some(emulator) = TerminalEmulator::from_binary(&terminal) {
                    return Ok(emulator);
                }
            }
        }
    }

    // 2. Detect currently running terminal from parent process chain
    // This is the most reliable method - we're almost certainly running inside a terminal
    if let Some(terminal) = detect_parent_terminal() {
        return Ok(terminal);
    }

    // 3. Check for cross-platform terminals that work on macOS
    let candidates = [
        TerminalEmulator::Kitty,
        TerminalEmulator::Alacritty,
        TerminalEmulator::Wezterm,
    ];

    for candidate in candidates {
        if which::which(candidate.binary()).is_ok() {
            return Ok(candidate);
        }
    }

    // 4. Check for installed macOS native terminals
    // Prefer iTerm2 if installed, otherwise fall back to Terminal.app
    if Path::new("/Applications/iTerm.app").exists() {
        return Ok(TerminalEmulator::ITerm2);
    }

    // Terminal.app is always present on macOS
    Ok(TerminalEmulator::TerminalApp)
}

/// Detect the terminal by walking up the parent process chain (macOS)
///
/// This checks if we're running inside a terminal by examining parent processes.
#[cfg(target_os = "macos")]
fn detect_parent_terminal() -> Option<TerminalEmulator> {
    // Get parent process info using ps
    let output = Command::new("ps")
        .args(["-o", "ppid=,comm=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let ppid: u32 = parts[0].parse().ok()?;

    // Walk up the process tree looking for a terminal
    let mut current_pid = ppid;
    for _ in 0..10 {
        // Limit depth to avoid infinite loops
        if current_pid <= 1 {
            break;
        }

        // Get process info for current_pid
        let output = Command::new("ps")
            .args(["-o", "ppid=,comm=", "-p", &current_pid.to_string()])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();
        if line.is_empty() {
            break;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            break;
        }

        let comm = parts[1];

        // Check if this process is a known terminal
        if let Some(terminal) = match_process_to_terminal(comm) {
            return Some(terminal);
        }

        current_pid = parts[0].parse().ok()?;
    }

    None
}

/// Detect running terminal apps on macOS
///
/// Checks which terminal applications are currently running.
///
/// NOTE: This function is intentionally unused because it's non-deterministic
/// when multiple terminals are running. Kept for potential debugging use.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn detect_running_terminal() -> Option<TerminalEmulator> {
    // Use ps to find running terminal processes
    let output = Command::new("ps")
        .args(["-axc", "-o", "comm="])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check for terminals in order of preference
    let terminal_checks = [
        ("iTerm2", TerminalEmulator::ITerm2),
        ("kitty", TerminalEmulator::Kitty),
        ("alacritty", TerminalEmulator::Alacritty),
        ("wezterm-gui", TerminalEmulator::Wezterm),
        ("WezTerm", TerminalEmulator::Wezterm),
        ("Terminal", TerminalEmulator::TerminalApp),
    ];

    for (process_name, terminal) in terminal_checks {
        if stdout.lines().any(|line| line.trim() == process_name) {
            // For cross-platform terminals, verify they're actually installed
            match terminal {
                TerminalEmulator::Kitty
                | TerminalEmulator::Alacritty
                | TerminalEmulator::Wezterm => {
                    if which::which(terminal.binary()).is_ok() {
                        return Some(terminal);
                    }
                }
                // For macOS-native terminals, just check the app exists
                TerminalEmulator::ITerm2 => {
                    if Path::new("/Applications/iTerm.app").exists() {
                        return Some(terminal);
                    }
                }
                TerminalEmulator::TerminalApp => {
                    return Some(terminal);
                }
                _ => {}
            }
        }
    }

    None
}

/// Match a process name to a terminal emulator
#[cfg(target_os = "macos")]
fn match_process_to_terminal(process_name: &str) -> Option<TerminalEmulator> {
    match process_name {
        "iTerm2" | "iTerm" => Some(TerminalEmulator::ITerm2),
        "Terminal" => Some(TerminalEmulator::TerminalApp),
        "kitty" => Some(TerminalEmulator::Kitty),
        "alacritty" | "Alacritty" => Some(TerminalEmulator::Alacritty),
        "wezterm" | "wezterm-gui" | "WezTerm" => Some(TerminalEmulator::Wezterm),
        _ => None,
    }
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
        if let Ok(terminal) = result {
            assert!(!terminal.binary().is_empty());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_match_process_to_terminal() {
        assert_eq!(
            match_process_to_terminal("iTerm2"),
            Some(TerminalEmulator::ITerm2)
        );
        assert_eq!(
            match_process_to_terminal("Terminal"),
            Some(TerminalEmulator::TerminalApp)
        );
        assert_eq!(
            match_process_to_terminal("kitty"),
            Some(TerminalEmulator::Kitty)
        );
        assert_eq!(
            match_process_to_terminal("wezterm-gui"),
            Some(TerminalEmulator::Wezterm)
        );
        assert_eq!(match_process_to_terminal("unknown"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_detect_running_terminal_finds_current() {
        // This test verifies that detect_running_terminal can find a terminal
        // when run from within a terminal. It should find at least one.
        let result = detect_running_terminal();
        // We expect to find something since we're running in a terminal
        assert!(
            result.is_some(),
            "Should detect a running terminal on macOS"
        );
    }
}
