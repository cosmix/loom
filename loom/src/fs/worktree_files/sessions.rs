//! Session file operations for worktrees

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::fs::session_files::enumerate_sessions;

// Re-export canonical implementation from fs::session_files
pub use crate::fs::session_files::find_sessions_for_stage;

/// Internal result for session cleanup
pub(crate) struct SessionCleanupResult {
    pub sessions_removed: usize,
    pub session_ids: Vec<String>,
    pub warnings: Vec<String>,
}

/// Clean up session files for a specific stage
pub(crate) fn cleanup_sessions_for_stage(
    stage_id: &str,
    work_dir: &Path,
    verbose: bool,
) -> Result<SessionCleanupResult> {
    let enumeration = enumerate_sessions(work_dir)?;
    let mut result = SessionCleanupResult {
        sessions_removed: 0,
        session_ids: Vec::new(),
        warnings: enumeration.diagnostics,
    };
    for record in enumeration.records {
        if record.stage_id.as_deref() != Some(stage_id) {
            continue;
        }
        match fs::remove_file(&record.path) {
            Ok(()) => {
                result.sessions_removed += 1;
                result.session_ids.push(record.id);
                if verbose {
                    println!("  Removed session file: {}", record.path.display());
                }
            }
            Err(error) => result.warnings.push(format!(
                "Failed to remove session file '{}': {error}",
                record.path.display()
            )),
        }
    }

    Ok(result)
}

/// Remove a single session file by session ID
pub fn remove_session_file(session_id: &str, work_dir: &Path) -> Result<bool> {
    let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));

    if !session_path.exists() {
        return Ok(false);
    }

    fs::remove_file(&session_path)
        .with_context(|| format!("Failed to remove session file: {}", session_path.display()))?;

    Ok(true)
}
