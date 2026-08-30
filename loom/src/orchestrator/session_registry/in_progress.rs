//! Strict discovery of persisted records that a stage takedown must retire.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::session::{Session, SessionStatus};
use crate::parser::frontmatter::parse_from_markdown;

/// Every persisted `Running` or `Spawning` record assigned to `stage_id`,
/// whether or not its process is still alive.
///
/// This recovery boundary is deliberately strict. Re-queueing after an
/// unreadable or malformed record could put a second writer in the worktree,
/// so uncertainty must keep the stage in `NeedsHandoff`.
pub fn in_progress_sessions_for_stage(work_dir: &Path, stage_id: &str) -> Result<Vec<Session>> {
    let sessions_dir = work_dir.join("sessions");
    let entries = match std::fs::read_dir(&sessions_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading session directory {}", sessions_dir.display()))
        }
    };

    let mut sessions = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("reading an entry from {}", sessions_dir.display()))?
            .path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let session = read_strict_session(&path)?;
        if belongs_in_takedown(&session, stage_id) {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

fn read_strict_session(path: &Path) -> Result<Session> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading persisted session {}", path.display()))?;
    let session = parse_from_markdown::<Session>(&content, "Session")
        .with_context(|| format!("parsing persisted session {}", path.display()))?;
    crate::fs::session_files::validate_session_file_id(&session.id)
        .with_context(|| format!("validating persisted session id in {}", path.display()))?;
    let file_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .with_context(|| {
            format!(
                "persisted session filename is not valid UTF-8: {}",
                path.display()
            )
        })?;
    anyhow::ensure!(
        file_id == session.id,
        "persisted session filename '{}' does not match record id '{}'",
        file_id,
        session.id
    );
    Ok(session)
}

fn belongs_in_takedown(session: &Session, stage_id: &str) -> bool {
    session.stage_id.as_deref() == Some(stage_id)
        && matches!(
            session.status,
            SessionStatus::Running | SessionStatus::Spawning
        )
}
