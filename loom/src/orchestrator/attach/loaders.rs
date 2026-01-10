//! Data loading functions for session attachment.

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::fs::stage_files::find_stage_file;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::terminal::BackendType;

use super::parsers;
use super::types::SessionBackend;

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
///
/// A session is attachable if:
/// - It has a tmux_session (tmux backend) OR a pid (native backend)
/// - It is in Running or Paused state
pub(crate) fn is_attachable(session: &Session) -> bool {
    // Must have either tmux session or PID
    let has_backend = session.tmux_session.is_some() || session.pid.is_some();
    if !has_backend {
        return false;
    }

    matches!(
        session.status,
        SessionStatus::Running | SessionStatus::Paused
    )
}

/// Determine the backend type for a session
///
/// If tmux_session is set, returns Tmux.
/// If only pid is set, returns Native.
pub(crate) fn detect_backend_type(session: &Session) -> Option<BackendType> {
    if session.tmux_session.is_some() {
        Some(BackendType::Tmux)
    } else if session.pid.is_some() {
        Some(BackendType::Native)
    } else {
        None
    }
}

/// Create a SessionBackend from a Session
pub(crate) fn session_backend(session: &Session) -> Option<SessionBackend> {
    if let Some(ref tmux_session) = session.tmux_session {
        Some(SessionBackend::Tmux {
            session_name: tmux_session.clone(),
        })
    } else {
        session.pid.map(|pid| SessionBackend::Native { pid })
    }
}
