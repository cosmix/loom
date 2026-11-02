//! Terminal emulator configuration
//!
//! Defines supported terminal emulators and their command-line interfaces.

use shell_escape::escape;
use std::borrow::Cow;
use std::path::Path;
use std::process::Command;

/// Escape a string for use in AppleScript double-quoted strings.
///
/// In AppleScript, within double quotes:
/// - Backslashes must be escaped: `\` → `\\`
/// - Double quotes must be escaped: `"` → `\"`
///
/// This prevents injection attacks where malicious stage IDs or commands
/// could break out of the AppleScript string context.
pub fn escape_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

/// Quote one executable path for APIs that accept shell text rather than argv.
fn shell_path(path: &Path) -> String {
    escape(Cow::Owned(path.to_string_lossy().into_owned())).into_owned()
}

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
    Ghostty,
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
            Self::Ghostty => "ghostty",
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
            "ghostty" => Some(Self::Ghostty),
            _ => None,
        }
    }

    /// Parse a terminal emulator from an application name (for macOS apps)
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Terminal" | "Terminal.app" | "Apple_Terminal" => Some(Self::TerminalApp),
            "iTerm" | "iTerm2" | "iTerm.app" | "iTerm2.app" => Some(Self::ITerm2),
            "Ghostty" | "Ghostty.app" | "ghostty" => Some(Self::Ghostty),
            // Fall back to binary matching for non-macOS terminals
            _ => Self::from_binary(name),
        }
    }

    /// Build a command to spawn this terminal with a title, working directory, and command to execute
    ///
    /// # Arguments
    /// * `title` - Window title for the terminal
    /// * `workdir` - Working directory for the command
    /// * `wrapper_path` - The executable wrapper path to run
    ///
    /// # Returns
    /// A configured Command ready to spawn
    pub fn build_command(&self, title: &str, workdir: &Path, wrapper_path: &Path) -> Command {
        let mut command = Command::new(self.binary());

        match self {
            Self::XdgTerminalExec => {
                command
                    .arg(format!("--title={title}"))
                    .arg(format!("--dir={}", workdir.display()))
                    .arg("--")
                    .arg(wrapper_path);
            }
            Self::Kitty => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--directory")
                    .arg(workdir)
                    .arg(wrapper_path);
            }
            Self::Alacritty => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg("-e")
                    .arg(wrapper_path);
            }
            Self::Foot => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg(wrapper_path);
            }
            Self::Wezterm => {
                command
                    .arg("start")
                    .arg("--cwd")
                    .arg(workdir)
                    .arg("--")
                    .arg(wrapper_path);
            }
            Self::GnomeTerminal => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg("--")
                    .arg(wrapper_path);
            }
            Self::Konsole => {
                command
                    .arg("--workdir")
                    .arg(workdir)
                    .arg("-e")
                    .arg(wrapper_path);
            }
            Self::Xfce4Terminal => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg("-x")
                    .arg(wrapper_path);
            }
            Self::MateTerminal => {
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg("-e")
                    .arg(shell_path(wrapper_path));
            }
            Self::XTerm => {
                command.arg("-title").arg(title).arg("-e").arg(wrapper_path);
            }
            Self::Urxvt => {
                command
                    .arg("-title")
                    .arg(title)
                    .arg("-cd")
                    .arg(workdir)
                    .arg("-e")
                    .arg(wrapper_path);
            }
            Self::TerminalApp => {
                // macOS Terminal.app uses AppleScript via osascript
                // Note: The wrapper script handles cd to the working directory,
                // so we just need to run the command. This is more reliable than
                // trying to cd in AppleScript, which can race with shell startup.
                //
                // SECURITY: Escape cmd and title to prevent AppleScript injection
                let escaped_cmd = escape_applescript_string(&shell_path(wrapper_path));
                let escaped_title = escape_applescript_string(title);
                let script = format!(
                    r#"tell application "Terminal"
    activate
    do script "{escaped_cmd}"
    set custom title of front window to "{escaped_title}"
end tell"#
                );
                command.arg("-e").arg(script);
            }
            Self::ITerm2 => {
                // macOS iTerm2 uses AppleScript via osascript
                // Note: The wrapper script handles cd to the working directory,
                // so we just need to run the command. Using `write text` can race
                // with shell startup, but since the wrapper script has the cd,
                // even if there's a delay the directory change will happen.
                //
                // SECURITY: Escape command and title to prevent AppleScript injection.
                let escaped_cmd = escape_applescript_string(&shell_path(wrapper_path));
                let escaped_title = escape_applescript_string(title);
                let script = format!(
                    r#"tell application "iTerm"
    activate
    set loomWindow to (create window with default profile)
    set name of loomWindow to "{escaped_title}"
    tell current session of loomWindow
        write text "{escaped_cmd}"
    end tell
end tell"#
                );
                command.arg("-e").arg(script);
            }
            Self::Ghostty => command = ghostty_command(title, workdir, wrapper_path),
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
            Self::Ghostty => "Ghostty",
        }
    }
}

fn ghostty_command(title: &str, workdir: &Path, wrapper_path: &Path) -> Command {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(not(target_os = "macos"))]
    let mut command = Command::new(TerminalEmulator::Ghostty.binary());
    #[cfg(target_os = "macos")]
    {
        // Ghostty's CLI binary lives inside /Applications/Ghostty.app/Contents/MacOS/
        // and is not on PATH by default (ghostty-org/ghostty#2483). Use `open -na`
        // which works without PATH setup and lets `--args` propagate to the new
        // process. -na (force new instance) ensures --working-directory and -e
        // take effect rather than being ignored by a running singleton.
        command
            .arg("-na")
            .arg("Ghostty")
            .arg("--args")
            .arg(format!("--working-directory={}", workdir.display()))
            .arg(format!("--title={}", title))
            .arg("-e")
            .arg(wrapper_path);
    }
    #[cfg(not(target_os = "macos"))]
    {
        command
            .arg("--title")
            .arg(title)
            .arg("--working-directory")
            .arg(workdir)
            .arg("-e")
            .arg(wrapper_path);
    }
    command
}

impl std::fmt::Display for TerminalEmulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests;
