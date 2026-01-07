//! Session attachment functionality for loom orchestrator.
//!
//! This module provides functionality to attach to running tmux sessions,
//! list attachable sessions, and manage multi-session views.

mod gui;
mod list;
mod overview;
mod parsers;
mod single;

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::fs::stage_files::find_stage_file;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;

// Re-export public API
pub use gui::{spawn_gui_windows, TerminalEmulator};
pub use list::{format_attachable_list, list_attachable};
pub use overview::{
    attach_overview_session, create_overview_session, create_tiled_overview,
    print_many_sessions_warning, print_overview_instructions, print_tiled_instructions,
};
pub use single::{attach_by_session, attach_by_stage, print_attach_instructions};

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

/// Load a session from .work/sessions/{id}.md
pub(crate) fn load_session(work_dir: &Path, session_id: &str) -> Result<Session> {
    let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));

    if !session_path.exists() {
        anyhow::bail!("Session file not found: {}", session_path.display());
    }

    let content = std::fs::read_to_string(&session_path)
        .with_context(|| format!("Failed to read session file: {}", session_path.display()))?;

    parsers::session_from_markdown(&content)
}

/// Load a stage from .work/stages/
pub(crate) fn load_stage(work_dir: &Path, stage_id: &str) -> Result<Stage> {
    let stages_dir = work_dir.join("stages");

    let stage_path = find_stage_file(&stages_dir, stage_id)?
        .ok_or_else(|| anyhow!("Stage file not found for: {stage_id}"))?;

    let content = std::fs::read_to_string(&stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

    parsers::stage_from_markdown(&content)
}

/// Find session for a stage
pub(crate) fn find_session_for_stage(work_dir: &Path, stage_id: &str) -> Result<Option<Session>> {
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
pub(crate) fn is_attachable(session: &Session) -> bool {
    if session.tmux_session.is_none() {
        return false;
    }

    matches!(
        session.status,
        SessionStatus::Running | SessionStatus::Paused
    )
}

/// Format session status for display
pub(crate) fn format_status(status: &SessionStatus) -> String {
    match status {
        SessionStatus::Spawning => "spawning".to_string(),
        SessionStatus::Running => "running".to_string(),
        SessionStatus::Paused => "paused".to_string(),
        SessionStatus::Completed => "completed".to_string(),
        SessionStatus::Crashed => "crashed".to_string(),
        SessionStatus::ContextExhausted => "exhausted".to_string(),
    }
}

/// Format a helpful error message for manual mode sessions
pub(crate) fn format_manual_mode_error(
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

/// Generate a window name from session info (truncated to 20 chars)
pub(crate) fn window_name_for_session(session: &AttachableSession) -> String {
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
///
/// Uses `env -u TMUX` to allow nested tmux sessions (running inside overview windows)
pub(crate) fn attach_command(tmux_session: &str, detach_existing: bool) -> String {
    if detach_existing {
        format!("env -u TMUX tmux attach -d -t {tmux_session}")
    } else {
        format!("env -u TMUX tmux attach -t {tmux_session}")
    }
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
