//! Common utility functions shared across command implementations.
//!
//! This module provides utilities for:
//! - State directory discovery
//! - Stage ID detection from worktree branch
//! - String truncation for display
//! - Shared tree-rendering helpers (see [`tree`])

pub(crate) mod model_display;
pub mod tree;

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::fs::work_dir::WorkDir;
use crate::git::branch::stage_id_from_branch;

/// Resolve the workspace containing the current directory.
///
/// Resolution — the order, the layout, and the bound on the upward walk — lives
/// in [`WorkDir::new`]; this only adds the existence check the commands need.
/// `WorkDir::new` always succeeds, falling back to a root that may not exist
/// yet, so a caller that requires a REAL workspace must reject that fallback.
pub fn resolve_work_dir() -> Result<WorkDir> {
    let work_dir = WorkDir::new(std::env::current_dir()?)?;
    if !work_dir.root().is_dir() {
        bail!("Could not find the .loom/work directory. Are you in a loom workspace?");
    }
    Ok(work_dir)
}

/// Path to the state directory of the workspace containing the current
/// directory, for callers that need nothing else from it.
pub fn work_dir_path() -> Result<PathBuf> {
    Ok(resolve_work_dir()?.root().to_path_buf())
}

/// Resolve the state directory rooted at `base`, without requiring it to
/// already exist.
///
/// Same resolution as [`resolve_work_dir`] (nested first, then a pre-move
/// legacy `.work/` workspace, falling back to the nested spelling for a base
/// with neither) but for an explicit `base` rather than the current
/// directory, and for callers — `loom clean --state`, `loom init`'s
/// pre-run cleanup, `loom repair` — that need a path to check `.exists()` on
/// rather than an error when the workspace is missing.
pub fn resolve_state_dir(base: &std::path::Path) -> PathBuf {
    WorkDir::new(base)
        .map(|wd| wd.root().to_path_buf())
        .unwrap_or_else(|_| base.join(".loom").join("work"))
}

/// Detect stage ID from current worktree branch.
///
/// Checks if the current git branch follows the loom worktree naming pattern
/// `loom/<stage-id>` and extracts the stage ID. Filters out special branches
/// like `loom/_base`.
pub fn detect_stage_id() -> Option<String> {
    detect_stage_id_in(&std::env::current_dir().ok()?)
}

/// Detect a stage branch in the repository containing `dir`.
pub fn detect_stage_id_in(dir: &Path) -> Option<String> {
    let git = crate::git::worktree::WorktreeGit::discovered(dir);
    let branch = crate::git::branch::checked_out_branch_with(&git)
        .ok()
        .flatten()?;

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
    use crate::test_git::TestRepo;

    #[test]
    fn stage_branch_with_same_named_tag_is_detected() {
        let repo = TestRepo::builder().branch("loom/s1").no_seed().create();
        repo.git(&["commit", "--allow-empty", "-qm", "seed"]);
        repo.git(&["tag", "loom/s1"]);
        assert_eq!(
            super::detect_stage_id_in(repo.path()).as_deref(),
            Some("s1")
        );
    }

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
