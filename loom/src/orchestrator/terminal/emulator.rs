//! Terminal emulator configuration
//!
//! Defines supported terminal emulators and their command-line interfaces.

use std::path::Path;
use std::process::Command;

/// Supported terminal emulators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalEmulator {
    XdgTerminalExec,
    Kitty,
    Alacritty,
    Foot,
    Wezterm,
    GnomeTerminal,
    Konsole,
    Xfce4Terminal,
    MateTerminal,
    XTerm,
    Urxvt,
    // macOS terminals
    TerminalApp,
    ITerm2,
}

impl TerminalEmulator {
    /// Get the binary name for this terminal
    pub fn binary(&self) -> &'static str {
        match self {
            Self::XdgTerminalExec => "xdg-terminal-exec",
            Self::Kitty => "kitty",
            Self::Alacritty => "alacritty",
            Self::Foot => "foot",
            Self::Wezterm => "wezterm",
            Self::GnomeTerminal => "gnome-terminal",
            Self::Konsole => "konsole",
            Self::Xfce4Terminal => "xfce4-terminal",
            Self::MateTerminal => "mate-terminal",
            Self::XTerm => "xterm",
            Self::Urxvt => "urxvt",
            // macOS terminals use osascript to launch via AppleScript
            Self::TerminalApp => "osascript",
            Self::ITerm2 => "osascript",
        }
    }

    /// Parse a terminal emulator from a binary name
    pub fn from_binary(binary: &str) -> Option<Self> {
        match binary {
            "xdg-terminal-exec" => Some(Self::XdgTerminalExec),
            "kitty" => Some(Self::Kitty),
            "alacritty" => Some(Self::Alacritty),
            "foot" => Some(Self::Foot),
            "wezterm" => Some(Self::Wezterm),
            "gnome-terminal" => Some(Self::GnomeTerminal),
            "konsole" => Some(Self::Konsole),
            "xfce4-terminal" => Some(Self::Xfce4Terminal),
            "mate-terminal" => Some(Self::MateTerminal),
            "xterm" => Some(Self::XTerm),
            "urxvt" => Some(Self::Urxvt),
            _ => None,
        }
    }

    /// Parse a terminal emulator from an application name (for macOS apps)
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Terminal" | "Terminal.app" => Some(Self::TerminalApp),
            "iTerm" | "iTerm2" | "iTerm.app" | "iTerm2.app" => Some(Self::ITerm2),
            // Fall back to binary matching for non-macOS terminals
            _ => Self::from_binary(name),
        }
    }

    /// Build a command to spawn this terminal with a title, working directory, and command to execute
    ///
    /// # Arguments
    /// * `title` - Window title for the terminal
    /// * `workdir` - Working directory for the command
    /// * `cmd` - The shell command to execute
    ///
    /// # Returns
    /// A configured Command ready to spawn
    pub fn build_command(&self, title: &str, workdir: &Path, cmd: &str) -> Command {
        let mut command = Command::new(self.binary());

        match self {
            Self::XdgTerminalExec => {
                command
                    .arg(format!("--title={title}"))
                    .arg(format!("--dir={}", workdir.display()))
                    .arg("--")
                    .arg("bash")
                    .arg("-c")
                    .arg(cmd);
            }
            Self::Kitty => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--directory")
                    .arg(workdir)
                    .arg("bash")
                    .arg("-c")
                    .arg(cmd);
            }
            Self::Alacritty => {
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
            Self::Foot => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg("bash")
                    .arg("-c")
                    .arg(cmd);
            }
            Self::Wezterm => {
                command
                    .arg("start")
                    .arg("--cwd")
                    .arg(workdir)
                    .arg("--")
                    .arg("bash")
                    .arg("-c")
                    .arg(cmd);
            }
            Self::GnomeTerminal => {
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
            Self::Konsole => {
                command
                    .arg("--workdir")
                    .arg(workdir)
                    .arg("-e")
                    .arg("bash")
                    .arg("-c")
                    .arg(cmd);
            }
            Self::Xfce4Terminal => {
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
            Self::MateTerminal => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg("-e")
                    .arg(format!("bash -c '{cmd}'"));
            }
            Self::XTerm => {
                command
                    .arg("-title")
                    .arg(title)
                    .arg("-e")
                    .arg("bash")
                    .arg("-c")
                    .arg(format!("cd {} && {}", workdir.display(), cmd));
            }
            Self::Urxvt => {
                command
                    .arg("-title")
                    .arg(title)
                    .arg("-cd")
                    .arg(workdir)
                    .arg("-e")
                    .arg("bash")
                    .arg("-c")
                    .arg(cmd);
            }
            Self::TerminalApp => {
                // macOS Terminal.app uses AppleScript via osascript
                let script = format!(
                    r#"tell application "Terminal"
    activate
    do script "cd '{}' && {}"
    set custom title of front window to "{}"
end tell"#,
                    workdir.display(),
                    cmd,
                    title
                );
                command.arg("-e").arg(script);
            }
            Self::ITerm2 => {
                // macOS iTerm2 uses AppleScript via osascript
                let script = format!(
                    r#"tell application "iTerm"
    activate
    create window with default profile
    tell current session of current window
        write text "cd '{}' && {}"
    end tell
end tell"#,
                    workdir.display(),
                    cmd
                );
                command.arg("-e").arg(script);
            }
        }

        command
    }

    /// Returns a human-readable name for this terminal emulator
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::XdgTerminalExec => "xdg-terminal-exec",
            Self::Kitty => "kitty",
            Self::Alacritty => "alacritty",
            Self::Foot => "foot",
            Self::Wezterm => "wezterm",
            Self::GnomeTerminal => "gnome-terminal",
            Self::Konsole => "konsole",
            Self::Xfce4Terminal => "xfce4-terminal",
            Self::MateTerminal => "mate-terminal",
            Self::XTerm => "xterm",
            Self::Urxvt => "urxvt",
            Self::TerminalApp => "Terminal.app",
            Self::ITerm2 => "iTerm2",
        }
    }
}

impl std::fmt::Display for TerminalEmulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_roundtrip() {
        // Linux terminals have unique binary names that roundtrip
        let terminals = [
            TerminalEmulator::XdgTerminalExec,
            TerminalEmulator::Kitty,
            TerminalEmulator::Alacritty,
            TerminalEmulator::Foot,
            TerminalEmulator::Wezterm,
            TerminalEmulator::GnomeTerminal,
            TerminalEmulator::Konsole,
            TerminalEmulator::Xfce4Terminal,
            TerminalEmulator::MateTerminal,
            TerminalEmulator::XTerm,
            TerminalEmulator::Urxvt,
        ];

        for terminal in terminals {
            let binary = terminal.binary();
            let parsed = TerminalEmulator::from_binary(binary);
            assert_eq!(parsed, Some(terminal));
        }
    }

    #[test]
    fn test_from_binary_unknown() {
        assert_eq!(TerminalEmulator::from_binary("unknown-terminal"), None);
    }

    #[test]
    fn test_from_name_macos() {
        // Test macOS Terminal.app variants
        assert_eq!(
            TerminalEmulator::from_name("Terminal"),
            Some(TerminalEmulator::TerminalApp)
        );
        assert_eq!(
            TerminalEmulator::from_name("Terminal.app"),
            Some(TerminalEmulator::TerminalApp)
        );

        // Test iTerm2 variants
        assert_eq!(
            TerminalEmulator::from_name("iTerm"),
            Some(TerminalEmulator::ITerm2)
        );
        assert_eq!(
            TerminalEmulator::from_name("iTerm2"),
            Some(TerminalEmulator::ITerm2)
        );
        assert_eq!(
            TerminalEmulator::from_name("iTerm.app"),
            Some(TerminalEmulator::ITerm2)
        );
        assert_eq!(
            TerminalEmulator::from_name("iTerm2.app"),
            Some(TerminalEmulator::ITerm2)
        );

        // from_name falls back to from_binary for Linux terminals
        assert_eq!(
            TerminalEmulator::from_name("kitty"),
            Some(TerminalEmulator::Kitty)
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(TerminalEmulator::Kitty.to_string(), "kitty");
        assert_eq!(
            TerminalEmulator::GnomeTerminal.to_string(),
            "gnome-terminal"
        );
        // macOS terminals display their app names, not "osascript"
        assert_eq!(TerminalEmulator::TerminalApp.to_string(), "Terminal.app");
        assert_eq!(TerminalEmulator::ITerm2.to_string(), "iTerm2");
    }

    #[test]
    fn test_macos_binary() {
        // Both macOS terminals use osascript
        assert_eq!(TerminalEmulator::TerminalApp.binary(), "osascript");
        assert_eq!(TerminalEmulator::ITerm2.binary(), "osascript");
    }

    #[test]
    fn test_terminal_app_build_command() {
        let emulator = TerminalEmulator::TerminalApp;
        let workdir = Path::new("/tmp/test");
        let cmd = emulator.build_command("Test Title", workdir, "echo hello");

        // Verify osascript is used
        assert_eq!(cmd.get_program(), "osascript");

        // Verify args contain the AppleScript
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-e");

        let script = args[1].to_str().unwrap();
        assert!(script.contains("tell application \"Terminal\""));
        assert!(script.contains("/tmp/test"));
        assert!(script.contains("echo hello"));
        assert!(script.contains("Test Title"));
    }

    #[test]
    fn test_iterm2_build_command() {
        let emulator = TerminalEmulator::ITerm2;
        let workdir = Path::new("/tmp/test");
        let cmd = emulator.build_command("Test Title", workdir, "echo hello");

        // Verify osascript is used
        assert_eq!(cmd.get_program(), "osascript");

        // Verify args contain the AppleScript
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-e");

        let script = args[1].to_str().unwrap();
        assert!(script.contains("tell application \"iTerm\""));
        assert!(script.contains("create window with default profile"));
        assert!(script.contains("/tmp/test"));
        assert!(script.contains("echo hello"));
    }
}
