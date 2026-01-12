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
        }

        command
    }
}

impl std::fmt::Display for TerminalEmulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.binary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_roundtrip() {
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
    fn test_display() {
        assert_eq!(TerminalEmulator::Kitty.to_string(), "kitty");
        assert_eq!(TerminalEmulator::GnomeTerminal.to_string(), "gnome-terminal");
    }
}
