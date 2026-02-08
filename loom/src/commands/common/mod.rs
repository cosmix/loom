//! Common utility functions shared across command implementations.
//!
//! This module provides utilities for:
//! - Work directory discovery
//! - Session ID detection (multiple strategies)
//! - Stage ID detection from various contexts
//! - String truncation for display

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::git::branch::stage_id_from_branch;

/// Find the .work directory by walking up from current directory.
///
/// Searches the current directory and all parent directories until it finds
/// a `.work` directory. This allows commands to work from any subdirectory
/// within a loom workspace.
pub fn find_work_dir() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let work_path = current.join(".work");
        if work_path.exists() && work_path.is_dir() {
            return Ok(work_path);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => bail!("Could not find .work directory. Are you in a loom workspace?"),
        }
    }
}

/// Detect session ID by finding most recent signal file.
///
/// This strategy:
/// 1. Checks LOOM_SESSION_ID environment variable
/// 2. Scans .work/signals/ for most recently modified signal file
///
/// Use this when you want to find the most recent active session.
pub fn detect_session_from_signals(work_dir: &Path) -> Result<String> {
    // Check environment variable first (set by loom when spawning)
    if let Ok(session_id) = std::env::var("LOOM_SESSION_ID") {
        return Ok(session_id);
    }

    // Try to detect from signal file in .work/signals/
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        bail!("No session ID provided or detected. Use --session <id>");
    }

    // Find most recent signal file
    let mut most_recent: Option<(String, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(&signals_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "md") {
            if let Some(stem) = path.file_stem() {
                let session_id = stem.to_string_lossy().to_string();
                let metadata = entry.metadata()?;
                let modified = metadata.modified()?;

                match &most_recent {
                    None => most_recent = Some((session_id, modified)),
                    Some((_, prev_time)) if modified > *prev_time => {
                        most_recent = Some((session_id, modified));
                    }
                    _ => {}
                }
            }
        }
    }

    most_recent
        .map(|(id, _)| id)
        .ok_or_else(|| anyhow::anyhow!("No session ID provided or detected. Use --session <id>"))
}

/// Detect stage ID from current worktree branch.
///
/// Checks if the current git branch follows the loom worktree naming pattern
/// `loom/<stage-id>` and extracts the stage ID. Filters out special branches
/// like `loom/_base`.
pub fn detect_stage_id() -> Option<String> {
    // Get current branch name
    let cwd = std::env::current_dir().ok()?;
    let output = crate::git::runner::run_git(&["rev-parse", "--abbrev-ref", "HEAD"], &cwd).ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Extract stage ID from branch name using centralized logic
    let stage_id = stage_id_from_branch(&branch)?;

    // Filter out special branches like _base
    if stage_id.starts_with('_') {
        return None;
    }

    Some(stage_id)
}

// Re-export truncate utilities from their canonical location in utils module.
// These are used across multiple layers (commands, orchestrator, verify, fs).
pub use crate::utils::{truncate, truncate_for_display};

#[cfg(test)]
mod tests {
    #[test]
    fn test_detect_stage_id_format() {
        let parse_branch = |branch: &str| -> Option<String> {
            branch.strip_prefix("loom/").and_then(|s| {
                if !s.starts_with('_') {
                    Some(s.to_string())
                } else {
                    None
                }
            })
        };

        assert_eq!(
            parse_branch("loom/implement-auth"),
            Some("implement-auth".to_string())
        );
        assert_eq!(
            parse_branch("loom/stage-123"),
            Some("stage-123".to_string())
        );
        assert_eq!(parse_branch("loom/_base"), None);
        assert_eq!(parse_branch("main"), None);
        assert_eq!(parse_branch("feature/test"), None);
    }
}
