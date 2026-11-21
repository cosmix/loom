//! Sticky fullscreen marker for one exact persisted session record.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::fs::locking::locked_update;
use crate::models::session::Session;
use crate::parser::frontmatter::parse_from_markdown;

use super::exact::{exact_session_path, validate_session_file_id};
use super::session_to_markdown;

/// Mark one exact persisted session as having shown a fullscreen (alternate
/// screen) pane. Sticky and status-agnostic: a session already marked terminal
/// still gets the flag when teardown is first to see its pane fullscreen.
/// Returns whether the record changed (false when already set or absent).
pub fn record_session_fullscreen_exact(work_dir: &Path, session_id: &str) -> Result<bool> {
    validate_session_file_id(session_id).context("Invalid session file id")?;
    let session_file = exact_session_path(work_dir, session_id);
    if !session_file
        .try_exists()
        .with_context(|| format!("checking session file: {}", session_file.display()))?
    {
        return Ok(false);
    }

    let mut changed = false;
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
        if current.fullscreen_seen {
            return Ok(content);
        }
        current.fullscreen_seen = true;
        changed = true;
        Ok(session_to_markdown(&current))
    })?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::session_files::{load_session_exact, save_session};
    use crate::models::session::SessionStatus;
    use tempfile::TempDir;

    #[test]
    fn the_fullscreen_flag_is_set_once_and_keeps_the_rest_of_the_record() {
        let temp_dir = TempDir::new().unwrap();
        let mut session = Session::new();
        session.status = SessionStatus::Completed;
        session.context_tokens = 5;
        save_session(&session, temp_dir.path()).unwrap();

        assert!(record_session_fullscreen_exact(temp_dir.path(), &session.id).unwrap());
        assert!(!record_session_fullscreen_exact(temp_dir.path(), &session.id).unwrap());
        assert!(!record_session_fullscreen_exact(temp_dir.path(), "session-absent").unwrap());

        let current = load_session_exact(temp_dir.path(), &session.id)
            .unwrap()
            .unwrap();
        assert!(current.fullscreen_seen);
        assert_eq!(
            (current.status, current.context_tokens),
            (SessionStatus::Completed, 5)
        );
    }
}
