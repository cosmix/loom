//! Common utility functions shared across command implementations.
//!
//! This module provides utilities for:
//! - Work directory discovery
//! - Stage ID detection from worktree branch
//! - String truncation for display

use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::git::branch::stage_id_from_branch;

/// Find the .work directory.
///
/// Resolution order:
///   1. `$LOOM_WORK_DIR` when it points at a directory that contains a
///      `.work`-shaped marker (`config.toml` or `stages/`). The
///      container backend relies on this to find `/repo/.work` instead
///      of walking the host filesystem.
///   2. Walk upward from the current directory.
pub fn find_work_dir() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;
    loop {
        let work_path = current.join(".work");
        if work_path.exists() && work_path.is_dir() {
            return Ok(work_path);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => {
                // Upward walk yielded nothing — honour LOOM_WORK_DIR
                // if validated. The container backend exports it at
                // `/repo/.work` so commands inside the container still
                // find state when invoked from a directory outside
                // /repo.
                if let Some(env_root) = work_dir_from_env() {
                    return Ok(env_root);
                }
                bail!("Could not find .work directory. Are you in a loom workspace?");
            }
        }
    }
}

/// Returns a validated `LOOM_WORK_DIR` if set **and** the path lives
/// under the container topology (`/repo/...`). See
/// `crate::fs::work_dir::loom_work_dir_from_env` for rationale.
fn work_dir_from_env() -> Option<PathBuf> {
    let raw = std::env::var_os("LOOM_WORK_DIR")?;
    let s = raw.to_str()?.trim();
    if s.is_empty() {
        return None;
    }
    if !s.starts_with("/repo/") && s != "/repo/.work" {
        return None;
    }
    let candidate = PathBuf::from(s);
    if !candidate.is_dir() {
        return None;
    }
    if candidate.join("config.toml").exists() || candidate.join("stages").is_dir() {
        Some(candidate)
    } else {
        None
    }
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
