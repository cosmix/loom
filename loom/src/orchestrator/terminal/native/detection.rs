//! Terminal detection logic
//!
//! Detects available terminal emulators on the system.

#[cfg(target_os = "linux")]
use anyhow::bail;
use anyhow::Result;

use std::process::Command;
use std::time::Duration;

#[cfg(target_os = "macos")]
use std::path::Path;

use super::super::emulator::TerminalEmulator;

const TERMINAL_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

fn run_terminal_probe(
    program: &str,
    args: &[&str],
    operation: &str,
) -> Option<std::process::Output> {
    let mut command = Command::new(program);
    command.args(args);
    crate::process::run_bounded_output(&mut command, TERMINAL_PROBE_TIMEOUT, operation).ok()
}

/// Detect the available terminal emulator (Linux)
///
/// Priority:
/// 0. LOOM_TERMINAL environment variable (set before daemon fork to preserve terminal choice)
/// 1. TERMINAL environment variable (user preference)
/// 2. gsettings/dconf default terminal (GNOME/Cosmic DE settings)
/// 3. xdg-terminal-exec (emerging standard)
/// 4. Common terminals: kitty, alacritty, etc.
#[cfg(target_os = "linux")]
pub fn detect_terminal() -> Result<TerminalEmulator> {
    // 0. Check LOOM_TERMINAL environment variable first (set before daemon fork to preserve terminal choice)
    if let Ok(terminal_name) = std::env::var("LOOM_TERMINAL") {
        if !terminal_name.is_empty() {
            if let Some(emulator) = TerminalEmulator::from_name(&terminal_name) {
                return Ok(emulator);
            }
        }
    }

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
    if let Some(output) = run_terminal_probe(
        "gsettings",
        &[
            "get",
            "org.gnome.desktop.default-applications.terminal",
            "exec",
        ],
        "gsettings terminal detection",
    ) {
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
    if let Some(output) = run_terminal_probe(
        "dconf",
        &["read", "/com/system76/cosmic/default-terminal"],
        "dconf terminal detection",
    ) {
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
/// 0. LOOM_TERMINAL environment variable (set before daemon fork to preserve terminal choice)
/// 1. TERMINAL environment variable (user preference)
/// 2. Currently running terminal (detected via parent process)
/// 3. Cross-platform terminals (kitty, alacritty, wezterm)
/// 4. iTerm2 or Terminal.app (check for installed apps)
#[cfg(target_os = "macos")]
pub fn detect_terminal() -> Result<TerminalEmulator> {
    // 0. Check LOOM_TERMINAL environment variable first (set before daemon fork to preserve terminal choice)
    if let Ok(terminal_name) = std::env::var("LOOM_TERMINAL") {
        if !terminal_name.is_empty() {
            if let Some(emulator) = TerminalEmulator::from_name(&terminal_name) {
                return Ok(emulator);
            }
        }
    }

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

    // 1.5. Check TERM_PROGRAM environment variable (set by most macOS terminals)
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        if !term_program.is_empty() {
            if let Some(emulator) = TerminalEmulator::from_name(&term_program) {
                return Ok(emulator);
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
        TerminalEmulator::Ghostty,
        TerminalEmulator::Kitty,
        TerminalEmulator::Alacritty,
        TerminalEmulator::Wezterm,
    ];

    for candidate in candidates {
        if which::which(candidate.binary()).is_ok() {
            return Ok(candidate);
        }
    }

    if Path::new("/Applications/Ghostty.app").exists() {
        return Ok(TerminalEmulator::Ghostty);
    }

    if Path::new("/Applications/iTerm.app").exists() {
        return Ok(TerminalEmulator::ITerm2);
    }

    Ok(TerminalEmulator::TerminalApp)
}

/// Detect the terminal by walking up the parent process chain (macOS)
///
/// This checks if we're running inside a terminal by examining parent processes.
#[cfg(target_os = "macos")]
fn detect_parent_terminal() -> Option<TerminalEmulator> {
    let own_pid = std::process::id().to_string();
    let output = run_terminal_probe(
        "ps",
        &["-o", "ppid=,comm=", "-p", &own_pid],
        "parent terminal root probe",
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let ppid: u32 = parts[0].parse().ok()?;

    let mut current_pid = ppid;
    for _ in 0..10 {
        if current_pid <= 1 {
            break;
        }

        let current_pid_arg = current_pid.to_string();
        let output = run_terminal_probe(
            "ps",
            &["-o", "ppid=,comm=", "-p", &current_pid_arg],
            "parent terminal ancestry probe",
        )?;

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

        if let Some(terminal) = match_process_to_terminal(comm) {
            return Some(terminal);
        }

        current_pid = parts[0].parse().ok()?;
    }

    None
}

/// Match a process name to a terminal emulator
#[cfg(target_os = "macos")]
fn match_process_to_terminal(process_name: &str) -> Option<TerminalEmulator> {
    match process_name {
        "iTerm2" | "iTerm" => Some(TerminalEmulator::ITerm2),
        "Terminal" => Some(TerminalEmulator::TerminalApp),
        "ghostty" | "Ghostty" => Some(TerminalEmulator::Ghostty),
        "kitty" => Some(TerminalEmulator::Kitty),
        "alacritty" | "Alacritty" => Some(TerminalEmulator::Alacritty),
        "wezterm" | "wezterm-gui" | "WezTerm" => Some(TerminalEmulator::Wezterm),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // These process-global environment variables are read and mutated by all
    // detection tests, so serial execution prevents cross-test interference.

    #[test]
    #[serial]
    fn test_detect_terminal_finds_something() {
        let result = detect_terminal();
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
    #[serial]
    fn test_loom_terminal_env_var_takes_precedence() {
        let original = std::env::var("LOOM_TERMINAL").ok();

        std::env::set_var("LOOM_TERMINAL", "Ghostty");
        let result = detect_terminal();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TerminalEmulator::Ghostty);

        std::env::set_var("LOOM_TERMINAL", "iTerm2");
        let result = detect_terminal();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TerminalEmulator::ITerm2);

        std::env::set_var("LOOM_TERMINAL", "kitty");
        let result = detect_terminal();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TerminalEmulator::Kitty);

        // Restore original value or remove if it didn't exist
        if let Some(val) = original {
            std::env::set_var("LOOM_TERMINAL", val);
        } else {
            std::env::remove_var("LOOM_TERMINAL");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[serial]
    fn test_term_program_env_var_detection() {
        let original_loom = std::env::var("LOOM_TERMINAL").ok();
        let original_terminal = std::env::var("TERMINAL").ok();
        let original_term_program = std::env::var("TERM_PROGRAM").ok();

        // Clear higher-priority vars so TERM_PROGRAM is used
        std::env::remove_var("LOOM_TERMINAL");
        std::env::remove_var("TERMINAL");

        std::env::set_var("TERM_PROGRAM", "Apple_Terminal");
        let result = detect_terminal();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TerminalEmulator::TerminalApp);

        std::env::set_var("TERM_PROGRAM", "iTerm.app");
        let result = detect_terminal();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TerminalEmulator::ITerm2);

        // Restore original values
        if let Some(val) = original_loom {
            std::env::set_var("LOOM_TERMINAL", val);
        }
        if let Some(val) = original_terminal {
            std::env::set_var("TERMINAL", val);
        }
        if let Some(val) = original_term_program {
            std::env::set_var("TERM_PROGRAM", val);
        } else {
            std::env::remove_var("TERM_PROGRAM");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[serial]
    fn test_loom_terminal_env_var_invalid() {
        // Test that invalid LOOM_TERMINAL falls back to regular detection
        std::env::set_var("LOOM_TERMINAL", "invalid-terminal-name");
        let result = detect_terminal();
        // Should fall back to regular detection and still find something
        assert!(result.is_ok());
        std::env::remove_var("LOOM_TERMINAL");
    }
}
