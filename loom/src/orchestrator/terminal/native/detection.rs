//! Terminal detection logic
//!
//! Detects available terminal emulators on the system.

use anyhow::{bail, Result};
use std::process::Command;

use super::super::emulator::TerminalEmulator;

/// Detect the available terminal emulator
///
/// Priority:
/// 1. TERMINAL environment variable (user preference)
/// 2. gsettings/dconf default terminal (GNOME/Cosmic DE settings)
/// 3. xdg-terminal-exec (emerging standard)
/// 4. Common terminals: kitty, alacritty, etc.
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
            assert!(!terminal.binary().is_empty());
        }
    }
}
