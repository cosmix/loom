//! Session cleanup utilities
//!
//! Note: Session finding functions (find_session_for_stage, find_sessions_for_stage)
//! are now in `crate::fs::session_files`. Import from there instead.

use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

use crate::fs::locking::locked_read;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::parser::frontmatter::parse_from_markdown;

/// Clean up resources associated with a completed stage
///
/// This function performs best-effort cleanup and logs warnings on failure:
/// 1. Updates session status to Completed
/// 2. Removes the signal file
pub fn cleanup_session_resources(_stage_id: &str, session_id: &str, work_dir: &Path) {
    // 1. Update session status to Completed
    if let Err(e) = update_session_status(work_dir, session_id, SessionStatus::Completed) {
        eprintln!("Warning: failed to update session status: {e}");
    }

    // 2. Remove signal file
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
    match fs::remove_file(&signal_path) {
        Ok(()) => {
            println!("Removed signal file '{}'", signal_path.display());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Signal file may not exist - this is fine
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to remove signal file '{}': {e}",
                signal_path.display()
            );
        }
    }
}

/// Update a session's status in .work/sessions/
///
/// Reads through the shared file lock (`locked_read`) and writes through the
/// canonical `save_session` (`locked_write` + atomic rename) so this CLI path
/// cannot race the daemon/monitor readers and writers that touch the same
/// session files under locks.
fn update_session_status(work_dir: &Path, session_id: &str, status: SessionStatus) -> Result<()> {
    let sessions_dir = work_dir.join("sessions");
    let session_path = sessions_dir.join(format!("{session_id}.md"));

    if !session_path.exists() {
        bail!("Session file not found: {}", session_path.display());
    }

    let content = locked_read(&session_path)?;

    // Parse session from markdown
    let mut session: Session = parse_from_markdown(&content, "Session")?;

    // Update status
    session.status = status;
    session.last_active = chrono::Utc::now();

    // Write back through the canonical locked + atomic path.
    save_session(&session, work_dir)?;

    Ok(())
}
