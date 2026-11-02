//! Session cleanup utilities
//!
//! Note: Session finding functions (find_session_for_stage, find_sessions_for_stage)
//! are now in `crate::fs::session_files`. Import from there instead.

use std::fs;
use std::path::Path;

use crate::fs::session_files::mark_session_terminal_reason;
use crate::models::session::{SessionExitReason, SessionStatus};

/// Clean up resources associated with a completed stage
///
/// This function performs best-effort cleanup and logs warnings on failure:
/// 1. Updates session status to Completed
/// 2. Removes the signal file
pub fn cleanup_session_resources(_stage_id: &str, session_id: &str, work_dir: &Path) {
    // 1. Update session status to Completed
    if let Err(e) = mark_session_terminal_reason(
        work_dir,
        session_id,
        SessionStatus::Completed,
        SessionExitReason::Completed,
    ) {
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
