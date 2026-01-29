//! Terminal emulator configuration
//!
//! Defines supported terminal emulators and their command-line interfaces.

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
fn escape_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Escape a string for use in single-quoted shell strings.
///
/// In shell single quotes, the only character that needs special handling
/// is the single quote itself, which cannot be escaped inside single quotes.
/// The standard approach is to end the single-quoted string, add an escaped
/// single quote, and start a new single-quoted string: ' → '\''
///
/// This prevents command injection when embedding untrusted input in shell commands.
fn escape_shell_single_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
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
                // SECURITY: Escape single quotes in cmd to prevent shell injection
                let escaped_cmd = escape_shell_single_quote(cmd);
                command
                    .arg("--title")
                    .arg(title)
                    .arg("--working-directory")
                    .arg(workdir)
                    .arg("-e")
                    .arg(format!("bash -c '{escaped_cmd}'"));
            }
            Self::XTerm => {
                // SECURITY: Escape the workdir path to prevent shell injection
                // Use single quotes around path and escape any single quotes in it
                let escaped_workdir = escape_shell_single_quote(&workdir.display().to_string());
                command
                    .arg("-title")
                    .arg(title)
                    .arg("-e")
                    .arg("bash")
                    .arg("-c")
                    .arg(format!("cd '{}' && {}", escaped_workdir, cmd));
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
                // Note: The wrapper script handles cd to the working directory,
                // so we just need to run the command. This is more reliable than
                // trying to cd in AppleScript, which can race with shell startup.
                //
                // SECURITY: Escape cmd and title to prevent AppleScript injection
                let escaped_cmd = escape_applescript_string(cmd);
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
                // SECURITY: Escape cmd to prevent AppleScript injection
                let escaped_cmd = escape_applescript_string(cmd);
                let script = format!(
                    r#"tell application "iTerm"
    activate
    create window with default profile
    tell current session of current window
        write text "{escaped_cmd}"
    end tell
end tell"#
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
        // Note: workdir is no longer in the AppleScript - it's handled by the wrapper script
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
        // Note: workdir is no longer in the AppleScript - it's handled by the wrapper script
        assert!(script.contains("echo hello"));
    }

    #[test]
    fn test_escape_applescript_string() {
        use super::escape_applescript_string;

        // Test basic escaping
        assert_eq!(escape_applescript_string("hello"), "hello");

        // Test quote escaping
        assert_eq!(escape_applescript_string(r#"say "hi""#), r#"say \"hi\""#);

        // Test backslash escaping
        assert_eq!(
            escape_applescript_string(r#"path\to\file"#),
            r#"path\\to\\file"#
        );

        // Test combined escaping
        assert_eq!(
            escape_applescript_string(r#"echo "test\path""#),
            r#"echo \"test\\path\""#
        );

        // Test potential injection attempt
        let malicious = r#""; do shell script "rm -rf /" --"#;
        let escaped = escape_applescript_string(malicious);
        // The quotes should be escaped, preventing breakout
        assert!(escaped.contains(r#"\""#));
        assert!(!escaped.contains(r#"" do"#)); // No unescaped quote followed by space
    }

    #[test]
    fn test_terminal_app_escapes_special_chars() {
        let emulator = TerminalEmulator::TerminalApp;
        let workdir = Path::new("/tmp");

        // Test with command containing quotes
        let cmd = emulator.build_command("Test", workdir, r#"echo "hello""#);
        let args: Vec<_> = cmd.get_args().collect();
        let script = args[1].to_str().unwrap();

        // The quotes in the command should be escaped
        assert!(script.contains(r#"echo \"hello\""#));
    }

    #[test]
    fn test_terminal_app_escapes_title() {
        let emulator = TerminalEmulator::TerminalApp;
        let workdir = Path::new("/tmp");

        // Test with title containing quotes (potential injection)
        let cmd = emulator.build_command(r#"Stage "test""#, workdir, "echo hi");
        let args: Vec<_> = cmd.get_args().collect();
        let script = args[1].to_str().unwrap();

        // The quotes in the title should be escaped
        assert!(script.contains(r#"Stage \"test\""#));
    }

    #[test]
    fn test_iterm2_escapes_special_chars() {
        let emulator = TerminalEmulator::ITerm2;
        let workdir = Path::new("/tmp");

        // Test with command containing backslashes and quotes
        let cmd = emulator.build_command("Test", workdir, r#"echo "path\to\file""#);
        let args: Vec<_> = cmd.get_args().collect();
        let script = args[1].to_str().unwrap();

        // Both quotes and backslashes should be escaped
        assert!(script.contains(r#"echo \"path\\to\\file\""#));
    }

    #[test]
    fn test_escape_shell_single_quote() {
        use super::escape_shell_single_quote;

        // Test no escaping needed
        assert_eq!(escape_shell_single_quote("hello"), "hello");

        // Test single quote escaping
        assert_eq!(escape_shell_single_quote("it's"), "it'\\''s");

        // Test multiple single quotes
        assert_eq!(escape_shell_single_quote("'test'"), "'\\''test'\\''");

        // Test potential injection attempt
        // Input: '; rm -rf /; echo '
        // Each ' becomes '\'' (end quote, escaped quote, start quote)
        let malicious = "'; rm -rf /; echo '";
        let escaped = escape_shell_single_quote(malicious);
        // Expected: '\''  +  ; rm -rf /; echo   +  '\''
        assert_eq!(escaped, "'\\''; rm -rf /; echo '\\''");
    }

    #[test]
    fn test_mate_terminal_escapes_command() {
        let emulator = TerminalEmulator::MateTerminal;
        let workdir = Path::new("/tmp");

        // Test with command containing single quotes (potential injection)
        let cmd = emulator.build_command("Test", workdir, "echo 'hello'");
        let args: Vec<_> = cmd.get_args().collect();
        let last_arg = args.last().unwrap().to_str().unwrap();

        // The single quotes should be escaped to prevent injection
        assert!(last_arg.contains("'\\''"));
        // Should still be wrapped in single quotes for bash -c
        assert!(last_arg.starts_with("bash -c '"));
    }

    #[test]
    fn test_xterm_escapes_workdir() {
        let emulator = TerminalEmulator::XTerm;
        // Test with workdir containing single quote (potential injection)
        let workdir = Path::new("/tmp/test's dir");
        let cmd = emulator.build_command("Test", workdir, "echo hello");
        let args: Vec<_> = cmd.get_args().collect();
        let last_arg = args.last().unwrap().to_str().unwrap();

        // The workdir should be single-quoted with escaping
        assert!(last_arg.contains("cd '"));
        // The single quote in the path should be escaped
        assert!(last_arg.contains("'\\''"));
    }
}
