//! GUI terminal emulator support for multi-session attachment.
//!
//! Provides functionality to spawn separate GUI terminal windows for
//! each loom session, using the detected terminal emulator.
//! Supports both tmux and native sessions.

use anyhow::{anyhow, Result};

use super::{tmux_attach_command, AttachableSession, SessionBackend};

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
///
/// For tmux sessions: spawns terminal with tmux attach command.
/// For native sessions: focuses existing windows instead of spawning new ones.
pub fn spawn_gui_windows(sessions: &[AttachableSession], detach_existing: bool) -> Result<()> {
    let terminal = TerminalEmulator::detect().ok_or_else(|| {
        anyhow!(
            "No supported terminal emulator found.\n\
             Supported: gnome-terminal, konsole, xfce4-terminal, mate-terminal, \
             alacritty, kitty, wezterm, xterm, urxvt"
        )
    })?;

    // Separate tmux and native sessions
    let tmux_sessions: Vec<_> = sessions.iter().filter(|s| s.is_tmux()).collect();
    let native_sessions: Vec<_> = sessions.iter().filter(|s| s.is_native()).collect();

    if !tmux_sessions.is_empty() {
        println!(
            "\nOpening {} tmux session(s) in {} windows...\n",
            tmux_sessions.len(),
            terminal
        );

        for session in &tmux_sessions {
            let title = format!(
                "loom: {}",
                session
                    .stage_name
                    .as_ref()
                    .or(session.stage_id.as_ref())
                    .unwrap_or(&session.session_id)
            );

            if let SessionBackend::Tmux { session_name } = &session.backend {
                let attach_cmd = tmux_attach_command(session_name, detach_existing);
                let mut cmd = terminal.spawn_with_command(&title, &attach_cmd);

                match cmd.spawn() {
                    Ok(_) => println!("  Opened: {session_name} ({title})"),
                    Err(e) => eprintln!("  Failed to open {session_name}: {e}"),
                }
            }
        }
    }

    if !native_sessions.is_empty() {
        println!(
            "\nFocusing {} native session(s)...\n",
            native_sessions.len()
        );

        for session in &native_sessions {
            let stage_display = session
                .stage_name
                .as_ref()
                .or(session.stage_id.as_ref())
                .map(|s| s.as_str())
                .unwrap_or(&session.session_id);

            if let SessionBackend::Native { pid } = &session.backend {
                if focus_window_by_pid(*pid) {
                    println!("  Focused: {stage_display} (PID: {pid})");
                } else {
                    eprintln!("  Could not focus: {stage_display} (PID: {pid})");
                }
            }
        }
    }

    let total = tmux_sessions.len() + native_sessions.len();
    println!("\nProcessed {total} session(s).");

    if !tmux_sessions.is_empty() {
        println!("Tip: Use 'loom attach --all' (without --gui) for a unified tmux view.");
    }

    Ok(())
}

/// Focus a window by PID (for native sessions in GUI mode)
fn focus_window_by_pid(pid: u32) -> bool {
    // Try wmctrl first
    if which::which("wmctrl").is_ok() {
        if let Ok(output) = std::process::Command::new("wmctrl")
            .arg("-l")
            .arg("-p")
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(window_pid) = parts[2].parse::<u32>() {
                        if window_pid == pid {
                            let window_id = parts[0];
                            if std::process::Command::new("wmctrl")
                                .args(["-i", "-a", window_id])
                                .output()
                                .is_ok()
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        if let Ok(output) = std::process::Command::new("xdotool")
            .args(["search", "--pid", &pid.to_string(), "windowactivate"])
            .output()
        {
            if output.status.success() {
                return true;
            }
        }
    }

    false
}
