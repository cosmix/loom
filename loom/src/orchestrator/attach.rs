use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;

use crate::fs::stage_files::find_stage_file;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::parser::markdown::MarkdownDocument;

/// Information about an attachable session
#[derive(Debug, Clone)]
pub struct AttachableSession {
    pub session_id: String,
    pub stage_id: Option<String>,
    pub stage_name: Option<String>,
    pub tmux_session: String,
    pub status: SessionStatus,
    pub context_percent: f64,
}

/// Attach to a tmux session by stage ID
/// - Looks up the session for the stage
/// - Prints helpful detach instructions first
/// - Executes: `tmux attach -t {session_name}`
/// - This will replace the current process (exec)
pub fn attach_by_stage(stage_id: &str, work_dir: &Path) -> Result<()> {
    let session = find_session_for_stage(work_dir, stage_id)?
        .ok_or_else(|| anyhow!("No active session found for stage '{stage_id}'"))?;

    let tmux_session = match session.tmux_session {
        Some(ref s) => s.clone(),
        None => {
            return Err(format_manual_mode_error(
                &session.id,
                session.worktree_path.as_ref(),
                work_dir,
            ));
        }
    };

    print_attach_instructions(&tmux_session);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(&tmux_session)
            .exec();
        Err(anyhow!("Failed to exec tmux: {error}"))
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(&tmux_session)
            .status()
            .context("Failed to execute tmux command")?;

        if !status.success() {
            bail!("tmux attach failed with status: {}", status);
        }
        Ok(())
    }
}

/// Attach to a tmux session directly by session ID or tmux session name
pub fn attach_by_session(session_id: &str, work_dir: &Path) -> Result<()> {
    let session = load_session(work_dir, session_id)?;

    let tmux_session = match session.tmux_session {
        Some(ref s) => s.clone(),
        None => {
            return Err(format_manual_mode_error(
                session_id,
                session.worktree_path.as_ref(),
                work_dir,
            ));
        }
    };

    print_attach_instructions(&tmux_session);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(&tmux_session)
            .exec();
        Err(anyhow!("Failed to exec tmux: {error}"))
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(&tmux_session)
            .status()
            .context("Failed to execute tmux command")?;

        if !status.success() {
            bail!("tmux attach failed with status: {}", status);
        }
        Ok(())
    }
}

/// List all sessions that can be attached to
/// - Reads .work/sessions/ for session files
/// - Filters to Running or Paused sessions with tmux_session set
/// - Returns list with context health information
pub fn list_attachable(work_dir: &Path) -> Result<Vec<AttachableSession>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut attachable = Vec::new();

    let entries = std::fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        match load_session(work_dir, &session_id) {
            Ok(session) => {
                if !is_attachable(&session) {
                    continue;
                }

                let tmux_session = session.tmux_session.clone().unwrap();
                let context_percent = session.context_health() as f64;

                let (stage_id, stage_name) = if let Some(ref sid) = session.stage_id {
                    match load_stage(work_dir, sid) {
                        Ok(stage) => (Some(sid.clone()), Some(stage.name)),
                        Err(_) => (Some(sid.clone()), None),
                    }
                } else {
                    (None, None)
                };

                attachable.push(AttachableSession {
                    session_id: session.id,
                    stage_id,
                    stage_name,
                    tmux_session,
                    status: session.status,
                    context_percent,
                });
            }
            Err(_) => {
                continue;
            }
        }
    }

    attachable.sort_by(|a, b| a.session_id.cmp(&b.session_id));

    Ok(attachable)
}

/// Print the pre-attach instructions message
/// Shows helpful info about detaching and scrolling
pub fn print_attach_instructions(session_name: &str) {
    // Truncate session name if too long to fit in the box
    let display_name = if session_name.len() > 32 {
        format!("{}...", &session_name[..29])
    } else {
        session_name.to_string()
    };

    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│  Attaching to session {display_name:<32}│");
    println!("│                                                         │");
    println!("│  To detach (return to loom): Press Ctrl+B then D        │");
    println!("│  To scroll: Ctrl+B then [ (exit scroll: q)              │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Format a helpful error message for manual mode sessions
fn format_manual_mode_error(
    session_id: &str,
    worktree_path: Option<&std::path::PathBuf>,
    work_dir: &Path,
) -> anyhow::Error {
    let worktree_hint = match worktree_path {
        Some(path) => format!("cd {}", path.display()),
        None => "cd .worktrees/<stage-id>".to_string(),
    };

    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
    let signal_hint = signal_path.display();

    anyhow!(
        "Session '{session_id}' was created in manual mode (no tmux session).\n\n\
         To work on this stage, navigate to the worktree manually:\n  \
         {worktree_hint}\n  \
         claude \"Read the signal file at {signal_hint} and execute the assigned work.\"\n"
    )
}

/// Generate the formatted table for `loom attach list`
pub fn format_attachable_list(sessions: &[AttachableSession]) -> String {
    let mut output = String::new();

    output.push_str("SESSION          STAGE              STATUS      CONTEXT\n");

    for session in sessions {
        let stage_display = session
            .stage_name
            .as_ref()
            .map(|s| {
                if s.len() > 18 {
                    format!("{}...", &s[..15])
                } else {
                    s.clone()
                }
            })
            .unwrap_or_else(|| "-".to_string());

        let status_display = format_status(&session.status);

        let session_display = if session.session_id.len() > 16 {
            format!("{}...", &session.session_id[..13])
        } else {
            session.session_id.clone()
        };

        output.push_str(&format!(
            "{session_display:<16} {stage_display:<18} {status_display:<11} {:>3.0}%\n",
            session.context_percent
        ));
    }

    output
}

/// Load a session from .work/sessions/{id}.md
fn load_session(work_dir: &Path, session_id: &str) -> Result<Session> {
    let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));

    if !session_path.exists() {
        bail!("Session file not found: {}", session_path.display());
    }

    let content = std::fs::read_to_string(&session_path)
        .with_context(|| format!("Failed to read session file: {}", session_path.display()))?;

    session_from_markdown(&content)
}

/// Load a stage from .work/stages/
fn load_stage(work_dir: &Path, stage_id: &str) -> Result<Stage> {
    let stages_dir = work_dir.join("stages");

    let stage_path = find_stage_file(&stages_dir, stage_id)?
        .ok_or_else(|| anyhow!("Stage file not found for: {stage_id}"))?;

    let content = std::fs::read_to_string(&stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

    stage_from_markdown(&content)
}

/// Find session for a stage
fn find_session_for_stage(work_dir: &Path, stage_id: &str) -> Result<Option<Session>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(None);
    }

    let entries = std::fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        match load_session(work_dir, &session_id) {
            Ok(session) => {
                if session.stage_id.as_deref() == Some(stage_id) {
                    return Ok(Some(session));
                }
            }
            Err(_) => continue,
        }
    }

    Ok(None)
}

/// Check if a session can be attached to
fn is_attachable(session: &Session) -> bool {
    if session.tmux_session.is_none() {
        return false;
    }

    matches!(
        session.status,
        SessionStatus::Running | SessionStatus::Paused
    )
}

/// Format session status for display
fn format_status(status: &SessionStatus) -> String {
    match status {
        SessionStatus::Spawning => "spawning".to_string(),
        SessionStatus::Running => "running".to_string(),
        SessionStatus::Paused => "paused".to_string(),
        SessionStatus::Completed => "completed".to_string(),
        SessionStatus::Crashed => "crashed".to_string(),
        SessionStatus::ContextExhausted => "exhausted".to_string(),
    }
}

/// Parse a Session from markdown content
fn session_from_markdown(content: &str) -> Result<Session> {
    use chrono::{DateTime, Utc};

    let doc =
        MarkdownDocument::parse(content).context("Failed to parse session markdown document")?;

    let id = doc
        .get_frontmatter("id")
        .ok_or_else(|| anyhow!("Missing 'id' in session frontmatter"))?
        .to_string();

    let stage_id = doc
        .get_frontmatter("stage_id")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != "null");

    let tmux_session = doc
        .get_frontmatter("tmux_session")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != "null");

    let worktree_path = doc
        .get_frontmatter("worktree_path")
        .filter(|s| !s.is_empty() && *s != "null")
        .map(std::path::PathBuf::from);

    let pid = doc
        .get_frontmatter("pid")
        .and_then(|s| s.parse::<u32>().ok());

    let status_str = doc
        .get_frontmatter("status")
        .ok_or_else(|| anyhow!("Missing 'status' in session frontmatter"))?;

    let status = match status_str.as_str() {
        "spawning" => SessionStatus::Spawning,
        "running" => SessionStatus::Running,
        "paused" => SessionStatus::Paused,
        "completed" => SessionStatus::Completed,
        "crashed" => SessionStatus::Crashed,
        "context_exhausted" => SessionStatus::ContextExhausted,
        _ => bail!("Invalid session status: {status_str}"),
    };

    let context_tokens = doc
        .get_frontmatter("context_tokens")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let context_limit = doc
        .get_frontmatter("context_limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(200_000);

    let created_at = doc
        .get_frontmatter("created_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| anyhow!("Missing or invalid 'created_at' in session frontmatter"))?;

    let last_active = doc
        .get_frontmatter("last_active")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| anyhow!("Missing or invalid 'last_active' in session frontmatter"))?;

    Ok(Session {
        id,
        stage_id,
        tmux_session,
        worktree_path,
        pid,
        status,
        context_tokens,
        context_limit,
        created_at,
        last_active,
    })
}

// =============================================================================
// Multi-session attachment support (attach --all)
// =============================================================================

/// Generate a window name from session info (truncated to 20 chars)
fn window_name_for_session(session: &AttachableSession) -> String {
    session
        .stage_name
        .clone()
        .or_else(|| session.stage_id.clone())
        .unwrap_or_else(|| session.session_id.clone())
        .chars()
        .take(20)
        .collect()
}

/// Build the tmux attach command string
fn attach_command(tmux_session: &str, detach_existing: bool) -> String {
    if detach_existing {
        format!("tmux attach -d -t {tmux_session}")
    } else {
        format!("tmux attach -t {tmux_session}")
    }
}

/// Create a tmux overview session with windows for each loom session
///
/// Each window runs `tmux attach -t <loom-session>` to connect to the
/// actual loom session. The overview session is named "loom-overview".
///
/// Uses `env -u TMUX` to handle running from within another tmux session.
pub fn create_overview_session(
    sessions: &[AttachableSession],
    detach_existing: bool,
) -> Result<String> {
    let overview_name = "loom-overview";

    // Kill existing overview session if it exists (ignore errors)
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", overview_name])
        .output();

    // Create the overview session with first loom session's window
    let first = &sessions[0];
    let first_window_name = window_name_for_session(first);
    let first_attach_cmd = attach_command(&first.tmux_session, detach_existing);

    // Use `env -u TMUX` to unset TMUX env var, allowing nested session creation
    let output = std::process::Command::new("env")
        .args([
            "-u",
            "TMUX",
            "tmux",
            "new-session",
            "-d",
            "-s",
            overview_name,
            "-n",
            &first_window_name,
            "sh",
            "-c",
            &first_attach_cmd,
        ])
        .output()
        .context("Failed to create overview session")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create overview session: {stderr}");
    }

    // Add remaining sessions as new windows
    for session in sessions.iter().skip(1) {
        let window_name = window_name_for_session(session);
        let attach_cmd = attach_command(&session.tmux_session, detach_existing);

        let output = std::process::Command::new("tmux")
            .args([
                "new-window",
                "-t",
                overview_name,
                "-n",
                &window_name,
                "sh",
                "-c",
                &attach_cmd,
            ])
            .output()
            .with_context(|| format!("Failed to add window for {}", session.session_id))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "Warning: Failed to add window for {}: {}",
                session.session_id, stderr
            );
        }
    }

    Ok(overview_name.to_string())
}

/// Print navigation instructions for the overview session
pub fn print_overview_instructions(session_count: usize) {
    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!(
        "│  loom Overview: {session_count} session(s)                              │"
    );
    println!("│                                                         │");
    println!("│  Navigate windows:                                      │");
    println!("│    Ctrl+B then N    - Next window                       │");
    println!("│    Ctrl+B then P    - Previous window                   │");
    println!("│    Ctrl+B then 0-9  - Jump to window by number          │");
    println!("│    Ctrl+B then W    - Window list                       │");
    println!("│                                                         │");
    println!("│  Detach (exit overview): Ctrl+B then D                  │");
    println!("│  Scroll in session:      Ctrl+B then [ (exit: q)        │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Attach to the overview session (replaces current process on Unix)
pub fn attach_overview_session(overview_name: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(overview_name)
            .exec();
        Err(anyhow!("Failed to exec tmux: {error}"))
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(overview_name)
            .status()
            .context("Failed to execute tmux command")?;

        if !status.success() {
            bail!("tmux attach failed with status: {}", status);
        }
        Ok(())
    }
}

// =============================================================================
// GUI terminal support (attach --all --gui)
// =============================================================================

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
        let command = match self {
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
        };
        command
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

// =============================================================================
// Helper parsers
// =============================================================================

/// Parse a Stage from markdown content
fn stage_from_markdown(content: &str) -> Result<Stage> {
    use crate::models::stage::StageStatus;
    use chrono::{DateTime, Utc};

    let doc =
        MarkdownDocument::parse(content).context("Failed to parse stage markdown document")?;

    let id = doc
        .get_frontmatter("id")
        .ok_or_else(|| anyhow!("Missing 'id' in stage frontmatter"))?
        .to_string();

    let name = doc
        .get_frontmatter("name")
        .ok_or_else(|| anyhow!("Missing 'name' in stage frontmatter"))?
        .to_string();

    let description = doc
        .get_frontmatter("description")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let status_str = doc
        .get_frontmatter("status")
        .ok_or_else(|| anyhow!("Missing 'status' in stage frontmatter"))?;

    let status = match status_str.as_str() {
        "pending" => StageStatus::Pending,
        "ready" => StageStatus::Ready,
        "executing" => StageStatus::Executing,
        "blocked" => StageStatus::Blocked,
        "completed" => StageStatus::Completed,
        "needs_handoff" => StageStatus::NeedsHandoff,
        "verified" => StageStatus::Verified,
        _ => bail!("Invalid stage status: {status_str}"),
    };

    let session = doc
        .get_frontmatter("session")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let created_at = doc
        .get_frontmatter("created_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(chrono::Utc::now);

    let updated_at = doc
        .get_frontmatter("updated_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(chrono::Utc::now);

    let completed_at = doc
        .get_frontmatter("completed_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    let close_reason = doc
        .get_frontmatter("close_reason")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    Ok(Stage {
        id,
        name,
        description,
        status,
        dependencies: Vec::new(),
        parallel_group: None,
        acceptance: Vec::new(),
        files: Vec::new(),
        plan_id: None,
        worktree: None,
        session,
        parent_stage: None,
        child_stages: Vec::new(),
        created_at,
        updated_at,
        completed_at,
        close_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_attachable_list() {
        let sessions = vec![
            AttachableSession {
                session_id: "session-1".to_string(),
                stage_id: Some("stage-1".to_string()),
                stage_name: Some("models".to_string()),
                tmux_session: "loom-session-1".to_string(),
                status: SessionStatus::Running,
                context_percent: 45.0,
            },
            AttachableSession {
                session_id: "session-2".to_string(),
                stage_id: Some("stage-2".to_string()),
                stage_name: Some("api".to_string()),
                tmux_session: "loom-session-2".to_string(),
                status: SessionStatus::Paused,
                context_percent: 23.5,
            },
        ];

        let output = format_attachable_list(&sessions);

        assert!(output.contains("SESSION"));
        assert!(output.contains("STAGE"));
        assert!(output.contains("STATUS"));
        assert!(output.contains("CONTEXT"));
        assert!(output.contains("session-1"));
        assert!(output.contains("session-2"));
        assert!(output.contains("models"));
        assert!(output.contains("api"));
        assert!(output.contains("running"));
        assert!(output.contains("paused"));
        assert!(output.contains("45%"));
        assert!(output.contains("24%"));
    }

    #[test]
    fn test_format_attachable_list_long_names() {
        let sessions = vec![AttachableSession {
            session_id: "very-long-session-identifier-name".to_string(),
            stage_id: Some("stage-1".to_string()),
            stage_name: Some("very-long-stage-name-that-exceeds-limit".to_string()),
            tmux_session: "loom-session-1".to_string(),
            status: SessionStatus::Running,
            context_percent: 75.8,
        }];

        let output = format_attachable_list(&sessions);

        assert!(output.contains("very-long-ses..."));
        assert!(output.contains("very-long-stage..."));
        assert!(output.contains("76%"));
    }

    #[test]
    fn test_print_attach_instructions() {
        print_attach_instructions("test-session");
    }

    #[test]
    fn test_context_percent_calculation() {
        let session = AttachableSession {
            session_id: "test".to_string(),
            stage_id: None,
            stage_name: None,
            tmux_session: "loom-test".to_string(),
            status: SessionStatus::Running,
            context_percent: 75.5,
        };

        assert_eq!(session.context_percent, 75.5);
    }

    #[test]
    fn test_attachable_filter() {
        use crate::models::session::Session;

        let mut running_session = Session::new();
        running_session.status = SessionStatus::Running;
        running_session.tmux_session = Some("tmux-1".to_string());

        let mut paused_session = Session::new();
        paused_session.status = SessionStatus::Paused;
        paused_session.tmux_session = Some("tmux-2".to_string());

        let mut completed_session = Session::new();
        completed_session.status = SessionStatus::Completed;
        completed_session.tmux_session = Some("tmux-3".to_string());

        let mut spawning_session = Session::new();
        spawning_session.status = SessionStatus::Spawning;
        spawning_session.tmux_session = Some("tmux-4".to_string());

        let mut no_tmux_session = Session::new();
        no_tmux_session.status = SessionStatus::Running;
        no_tmux_session.tmux_session = None;

        assert!(is_attachable(&running_session));
        assert!(is_attachable(&paused_session));
        assert!(!is_attachable(&completed_session));
        assert!(!is_attachable(&spawning_session));
        assert!(!is_attachable(&no_tmux_session));
    }

    #[test]
    fn test_format_status() {
        assert_eq!(format_status(&SessionStatus::Running), "running");
        assert_eq!(format_status(&SessionStatus::Paused), "paused");
        assert_eq!(format_status(&SessionStatus::Completed), "completed");
        assert_eq!(format_status(&SessionStatus::Crashed), "crashed");
        assert_eq!(format_status(&SessionStatus::ContextExhausted), "exhausted");
        assert_eq!(format_status(&SessionStatus::Spawning), "spawning");
    }

    #[test]
    fn test_session_from_markdown() {
        let markdown = r#"---
id: session-123
stage_id: stage-456
tmux_session: loom-session-123
status: running
context_tokens: 45000
context_limit: 200000
created_at: 2026-01-06T12:00:00Z
last_active: 2026-01-06T13:30:00Z
---

# Session: session-123
"#;

        let session = session_from_markdown(markdown).unwrap();
        assert_eq!(session.id, "session-123");
        assert_eq!(session.stage_id, Some("stage-456".to_string()));
        assert_eq!(session.tmux_session, Some("loom-session-123".to_string()));
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.context_tokens, 45000);
        assert_eq!(session.context_limit, 200000);
    }

    #[test]
    fn test_stage_from_markdown() {
        let markdown = r#"---
id: stage-123
name: Test Stage
description: A test stage
status: executing
session: session-456
created_at: 2026-01-06T12:00:00Z
updated_at: 2026-01-06T13:30:00Z
---

# Stage: Test Stage
"#;

        let stage = stage_from_markdown(markdown).unwrap();
        assert_eq!(stage.id, "stage-123");
        assert_eq!(stage.name, "Test Stage");
        assert_eq!(stage.description, Some("A test stage".to_string()));
        assert_eq!(stage.status, crate::models::stage::StageStatus::Executing);
        assert_eq!(stage.session, Some("session-456".to_string()));
    }

    #[test]
    fn test_format_manual_mode_error_with_worktree() {
        let work_dir = std::path::Path::new("/project/.work");
        let worktree_path = std::path::PathBuf::from("/project/.worktrees/stage-1");
        let error = format_manual_mode_error("session-123", Some(&worktree_path), work_dir);

        let error_msg = error.to_string();
        assert!(error_msg.contains("session-123"));
        assert!(error_msg.contains("manual mode"));
        assert!(error_msg.contains("cd /project/.worktrees/stage-1"));
        assert!(error_msg.contains("signals/session-123.md"));
    }

    #[test]
    fn test_format_manual_mode_error_without_worktree() {
        let work_dir = std::path::Path::new("/project/.work");
        let error = format_manual_mode_error("session-456", None, work_dir);

        let error_msg = error.to_string();
        assert!(error_msg.contains("session-456"));
        assert!(error_msg.contains("manual mode"));
        assert!(error_msg.contains("cd .worktrees/<stage-id>"));
        assert!(error_msg.contains("signals/session-456.md"));
    }

    #[test]
    fn test_print_attach_instructions_long_name() {
        // Should not panic with a very long session name
        print_attach_instructions("this-is-a-very-long-tmux-session-name-that-exceeds-32-chars");
    }
}
