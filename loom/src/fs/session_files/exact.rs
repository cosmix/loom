//! Exact-identity session reads and locked terminal-status updates.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use crate::fs::locking::{locked_read, locked_update};
use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::parser::frontmatter::parse_from_markdown;

use super::session_to_markdown;

fn exact_session_path(work_dir: &Path, session_id: &str) -> PathBuf {
    work_dir.join("sessions").join(format!("{session_id}.md"))
}

/// Validate a complete session id before using it as one path component.
pub(crate) fn validate_session_file_id(session_id: &str) -> Result<()> {
    // A common filesystem component limit is 255 bytes; reserve `.md`.
    if session_id.is_empty() || session_id.len() > 252 {
        bail!("Invalid session id length: {} bytes", session_id.len());
    }
    if !session_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        bail!(
            "Invalid session id '{}': use only ASCII alphanumeric characters, dashes, and underscores",
            session_id
        );
    }
    Ok(())
}

/// Load the exact persisted record for `session_id` under the session-file
/// lock. This never prefix-matches because daemon event identity is complete.
pub fn load_session_exact(work_dir: &Path, session_id: &str) -> Result<Option<Session>> {
    validate_session_file_id(session_id).context("Invalid session file id")?;
    let session_file = exact_session_path(work_dir, session_id);
    if !session_file
        .try_exists()
        .with_context(|| format!("checking session file: {}", session_file.display()))?
    {
        return Ok(None);
    }

    let content = locked_read(&session_file)
        .with_context(|| format!("reading session file: {}", session_file.display()))?;
    let session: Session = parse_from_markdown(&content, "Session")
        .with_context(|| format!("parsing session file: {}", session_file.display()))?;
    if session.id != session_id {
        bail!(
            "Session file {} contains id '{}', expected '{}'",
            session_file.display(),
            session.id,
            session_id
        );
    }
    Ok(Some(session))
}

/// Persist an exact context observation without replacing newer lifecycle or
/// backend fields in the session record.
pub fn record_session_context_exact(
    work_dir: &Path,
    session_id: &str,
    stage_id: &str,
    context_tokens: u32,
) -> Result<bool> {
    validate_session_file_id(session_id).context("Invalid session file id")?;
    let session_file = exact_session_path(work_dir, session_id);
    if !session_file
        .try_exists()
        .with_context(|| format!("checking session file: {}", session_file.display()))?
    {
        return Ok(false);
    }

    locked_update(&session_file, |content| {
        let mut current: Session = parse_from_markdown(&content, "Session")
            .with_context(|| format!("parsing session file: {}", session_file.display()))?;
        if current.id != session_id || current.stage_id.as_deref() != Some(stage_id) {
            bail!(
                "Session file {} contains identity ('{}', {:?}), expected ('{}', '{}')",
                session_file.display(),
                current.id,
                current.stage_id,
                session_id,
                stage_id
            );
        }
        current.context_tokens = context_tokens;
        Ok(session_to_markdown(&current))
    })?;
    Ok(true)
}

/// Apply a heartbeat to one exact live session as a locked read-modify-write.
/// Prefix ids cannot select a record, and a terminal transition that wins the
/// lock is never overwritten back to a stale `Running` snapshot.
pub fn record_session_heartbeat_exact(
    work_dir: &Path,
    session_id: &str,
    stage_id: &str,
    progress_at: DateTime<Utc>,
    context_tokens: Option<u32>,
    transcript_path: Option<String>,
) -> Result<bool> {
    validate_session_file_id(session_id).context("Invalid session file id")?;
    let session_file = exact_session_path(work_dir, session_id);
    if !session_file
        .try_exists()
        .with_context(|| format!("checking session file: {}", session_file.display()))?
    {
        return Ok(false);
    }

    let mut applied = false;
    locked_update(&session_file, |content| {
        let mut current: Session = parse_from_markdown(&content, "Session")
            .with_context(|| format!("parsing session file: {}", session_file.display()))?;
        if current.id != session_id {
            bail!(
                "Session file {} contains id '{}', expected '{}'",
                session_file.display(),
                current.id,
                session_id
            );
        }
        if current.status != SessionStatus::Running || current.stage_id.as_deref() != Some(stage_id)
        {
            return Ok(content);
        }
        current.record_heartbeat(progress_at, context_tokens, transcript_path);
        applied = true;
        Ok(session_to_markdown(&current))
    })?;
    Ok(applied)
}

/// Mark one exact persisted session terminal under its file lock.
///
/// A terminal record keeps its status. In every state, the first recorded
/// reason wins; this makes delayed generic events safe to replay.
pub fn mark_session_terminal_reason(
    work_dir: &Path,
    session_id: &str,
    status: SessionStatus,
    reason: SessionExitReason,
) -> Result<()> {
    if !status.is_terminal() {
        bail!("Terminal session update requires a terminal status");
    }
    validate_session_file_id(session_id).context("Invalid session file id")?;
    let session_file = exact_session_path(work_dir, session_id);
    if !session_file
        .try_exists()
        .with_context(|| format!("checking session file: {}", session_file.display()))?
    {
        bail!("Session file not found: {}", session_file.display());
    }

    locked_update(&session_file, |content| {
        let mut current: Session = parse_from_markdown(&content, "Session")
            .with_context(|| format!("parsing session file: {}", session_file.display()))?;
        if current.id != session_id {
            bail!(
                "Session file {} contains id '{}', expected '{}'",
                session_file.display(),
                current.id,
                session_id
            );
        }
        if !current.status.is_terminal() {
            current.try_mark_terminal(status.clone(), reason)?;
        } else if current.exit_reason.is_none() {
            current.exit_reason = Some(reason);
        }
        Ok(session_to_markdown(&current))
    })
    .with_context(|| {
        format!(
            "marking session '{}' terminal in {}",
            session_id,
            session_file.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::session_files::save_session;
    use tempfile::TempDir;

    #[test]
    fn context_exhausted_update_preserves_fresh_persisted_fields() {
        let temp_dir = TempDir::new().unwrap();
        let mut session = Session::new();
        session.status = SessionStatus::Running;
        session.context_tokens = 123_456;
        session.transcript_path = Some("/tmp/fresh.jsonl".to_string());
        session.pid = Some(42);
        save_session(&session, temp_dir.path()).unwrap();

        mark_session_terminal_reason(
            temp_dir.path(),
            &session.id,
            SessionStatus::ContextExhausted,
            SessionExitReason::ContextCeiling,
        )
        .unwrap();
        let current = load_session_exact(temp_dir.path(), &session.id)
            .unwrap()
            .unwrap();

        assert_eq!(current.status, SessionStatus::ContextExhausted);
        assert_eq!(current.exit_reason, Some(SessionExitReason::ContextCeiling));
        assert_eq!(current.context_tokens, 123_456);
        assert_eq!(current.transcript_path.as_deref(), Some("/tmp/fresh.jsonl"));
        assert_eq!(current.pid, Some(42));
    }

    #[test]
    fn context_exhausted_update_terminalizes_a_spawning_session() {
        let temp_dir = TempDir::new().unwrap();
        let session = Session::new();
        save_session(&session, temp_dir.path()).unwrap();

        mark_session_terminal_reason(
            temp_dir.path(),
            &session.id,
            SessionStatus::ContextExhausted,
            SessionExitReason::ContextCeiling,
        )
        .unwrap();
        let current = load_session_exact(temp_dir.path(), &session.id)
            .unwrap()
            .unwrap();

        assert_eq!(
            (current.status, current.exit_reason),
            (
                SessionStatus::ContextExhausted,
                Some(SessionExitReason::ContextCeiling)
            )
        );
    }

    #[test]
    fn context_exhausted_update_never_downgrades_terminal_status() {
        let temp_dir = TempDir::new().unwrap();
        let mut session = Session::new();
        session.status = SessionStatus::Completed;
        save_session(&session, temp_dir.path()).unwrap();
        mark_session_terminal_reason(
            temp_dir.path(),
            &session.id,
            SessionStatus::ContextExhausted,
            SessionExitReason::ContextCeiling,
        )
        .unwrap();

        let current = load_session_exact(temp_dir.path(), &session.id)
            .unwrap()
            .unwrap();
        assert_eq!(current.status, SessionStatus::Completed);
        assert_eq!(current.exit_reason, Some(SessionExitReason::ContextCeiling));
    }

    #[test]
    fn heartbeat_requires_an_exact_id_and_preserves_terminal_state() {
        let temp_dir = TempDir::new().unwrap();
        let mut session = Session::new();
        session.id = "session-exact-long".to_string();
        session.assign_to_stage("stage-1".to_string());
        session.status = SessionStatus::Completed;
        session.context_tokens = 10;
        save_session(&session, temp_dir.path()).unwrap();

        assert!(!record_session_heartbeat_exact(
            temp_dir.path(),
            "session-exact",
            "stage-1",
            session.last_active,
            Some(99),
            None,
        )
        .unwrap());
        assert!(!record_session_heartbeat_exact(
            temp_dir.path(),
            &session.id,
            "stage-1",
            session.last_active,
            Some(99),
            None,
        )
        .unwrap());
        let current = load_session_exact(temp_dir.path(), &session.id)
            .unwrap()
            .unwrap();
        assert_eq!(current.status, SessionStatus::Completed);
        assert_eq!(current.context_tokens, 10);
    }

    #[test]
    fn reordered_heartbeat_preserves_progress_but_applies_observation_facts() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join("reordered-heartbeat");
        let mut session = Session::new();
        session.id = "session-reordered".to_string();
        session.assign_to_stage("stage-reordered".to_string());
        session.status = SessionStatus::Running;
        let newest = Utc::now();
        session.last_active = newest;
        save_session(&session, &work_dir).unwrap();

        let applied = record_session_heartbeat_exact(
            &work_dir,
            &session.id,
            "stage-reordered",
            newest - chrono::Duration::minutes(5),
            Some(77_000),
            Some("/tmp/reordered.jsonl".to_string()),
        )
        .unwrap();
        let current = load_session_exact(&work_dir, &session.id).unwrap().unwrap();

        assert_eq!(
            (
                applied,
                current.last_active,
                current.context_tokens,
                current.transcript_path
            ),
            (
                true,
                newest,
                77_000,
                Some("/tmp/reordered.jsonl".to_string())
            )
        );
    }

    #[test]
    fn exact_session_updates_reject_path_traversal_before_touching_disk() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join("work");
        let outside = temp_dir.path().join("outside.md");
        std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
        std::fs::write(&outside, "sentinel").unwrap();

        let error = mark_session_terminal_reason(
            &work_dir,
            "../outside",
            SessionStatus::ContextExhausted,
            SessionExitReason::ContextCeiling,
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("Invalid session file id"));
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "sentinel");
    }
}
