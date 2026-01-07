//! GUI terminal emulator support for multi-session attachment.
//!
//! Provides functionality to spawn separate GUI terminal windows for
//! each loom session, using the detected terminal emulator.

use anyhow::{anyhow, Result};

use super::{attach_command, AttachableSession};

/// Supported terminal emulators for GUI mode
#[derive(Debug, Clone, Copy)]
pub enum TerminalEmulator {
    GnomeTerminal,
    Konsole,
    Xfce4Terminal,
    MateTerminal,
    Alacritty,
    Kitty,
    Wezterm,
    XTerm,
    Urxvt,
}

impl TerminalEmulator {
    /// Detect available terminal emulator on the system
    pub fn detect() -> Option<Self> {
        let candidates = [
            ("gnome-terminal", Self::GnomeTerminal),
            ("konsole", Self::Konsole),
            ("xfce4-terminal", Self::Xfce4Terminal),
            ("mate-terminal", Self::MateTerminal),
            ("alacritty", Self::Alacritty),
            ("kitty", Self::Kitty),
            ("wezterm", Self::Wezterm),
            ("xterm", Self::XTerm),
            ("urxvt", Self::Urxvt),
        ];

        for (cmd, emulator) in candidates {
            if std::process::Command::new("which")
                .arg(cmd)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return Some(emulator);
            }
        }

        None
    }

    /// Get the binary name for this terminal
    fn binary(&self) -> &'static str {
        match self {
            Self::GnomeTerminal => "gnome-terminal",
            Self::Konsole => "konsole",
            Self::Xfce4Terminal => "xfce4-terminal",
            Self::MateTerminal => "mate-terminal",
            Self::Alacritty => "alacritty",
            Self::Kitty => "kitty",
            Self::Wezterm => "wezterm",
            Self::XTerm => "xterm",
            Self::Urxvt => "urxvt",
        }
    }

    /// Build command to spawn terminal with given title and command
    pub fn spawn_with_command(&self, title: &str, cmd: &str) -> std::process::Command {
        match self {
            Self::GnomeTerminal => {
                let mut c = std::process::Command::new("gnome-terminal");
                c.args(["--title", title, "--", "sh", "-c", cmd]);
                c
            }
            Self::Konsole => {
                let mut c = std::process::Command::new("konsole");
                c.args(["-p", &format!("tabtitle={title}"), "-e", "sh", "-c", cmd]);
                c
            }
            Self::Xfce4Terminal => {
                let mut c = std::process::Command::new("xfce4-terminal");
                c.args(["--title", title, "-e", &format!("sh -c '{cmd}'")]);
                c
            }
            Self::MateTerminal => {
                let mut c = std::process::Command::new("mate-terminal");
                c.args(["--title", title, "-e", &format!("sh -c '{cmd}'")]);
                c
            }
            Self::Alacritty => {
                let mut c = std::process::Command::new("alacritty");
                c.args(["--title", title, "-e", "sh", "-c", cmd]);
                c
            }
            Self::Kitty => {
                let mut c = std::process::Command::new("kitty");
                c.args(["--title", title, "sh", "-c", cmd]);
                c
            }
            Self::Wezterm => {
                let mut c = std::process::Command::new("wezterm");
                c.args(["start", "--", "sh", "-c", cmd]);
                c
            }
            Self::XTerm => {
                let mut c = std::process::Command::new("xterm");
                c.args(["-title", title, "-e", cmd]);
                c
            }
            Self::Urxvt => {
                let mut c = std::process::Command::new("urxvt");
                c.args(["-title", title, "-e", "sh", "-c", cmd]);
                c
            }
        }
    }
}

impl std::fmt::Display for TerminalEmulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.binary())
    }
}

/// Spawn GUI terminal windows for each session
pub fn spawn_gui_windows(sessions: &[AttachableSession], detach_existing: bool) -> Result<()> {
    let terminal = TerminalEmulator::detect().ok_or_else(|| {
        anyhow!(
            "No supported terminal emulator found.\n\
             Supported: gnome-terminal, konsole, xfce4-terminal, mate-terminal, \
             alacritty, kitty, wezterm, xterm, urxvt"
        )
    })?;

    println!(
        "\nOpening {} session(s) in {} windows...\n",
        sessions.len(),
        terminal
    );

    for session in sessions {
        let title = format!(
            "loom: {}",
            session
                .stage_name
                .as_ref()
                .or(session.stage_id.as_ref())
                .unwrap_or(&session.session_id)
        );

        let attach_cmd = attach_command(&session.tmux_session, detach_existing);

        let mut cmd = terminal.spawn_with_command(&title, &attach_cmd);

        match cmd.spawn() {
            Ok(_) => println!("  Opened: {} ({})", session.tmux_session, title),
            Err(e) => eprintln!("  Failed to open {}: {}", session.tmux_session, e),
        }
    }

    println!("\nOpened {} terminal window(s).", sessions.len());
    println!("Tip: Use 'loom attach --all' (without --gui) for a unified tmux view.");

    Ok(())
}
