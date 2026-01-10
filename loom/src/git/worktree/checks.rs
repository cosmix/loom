//! Worktree validation checks
//!
//! Provides functions for checking git availability, worktree support,
//! and worktree validity.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use super::operations::list_worktrees;

/// Check if git is available
pub fn check_git_available() -> Result<()> {
    let output = Command::new("git")
        .args(["--version"])
        .output()
        .with_context(|| "Git is not installed or not in PATH")?;

    if !output.status.success() {
        bail!("Git is not working properly");
    }

    Ok(())
}

/// Check if git worktree is supported (git 2.15+)
pub fn check_worktree_support() -> Result<()> {
    check_git_available()?;

    let output = Command::new("git").args(["worktree", "list"]).output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => bail!("Git worktree feature not supported. Requires git 2.15+"),
    }
}

/// Check if a worktree exists for a stage
pub fn worktree_exists(stage_id: &str, repo_root: &Path) -> bool {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    worktree_path.exists()
}

/// Check if a path is a valid git worktree tracked by the repository
pub fn is_valid_git_worktree(worktree_path: &Path, repo_root: &Path) -> Result<bool> {
    let worktrees = list_worktrees(repo_root)?;

    // Canonicalize paths for comparison to handle symlinks and relative paths
    let worktree_canonical = worktree_path.canonicalize().ok();

    for wt in worktrees {
        let wt_canonical = wt.path.canonicalize().ok();

        // Compare canonical paths if available, otherwise compare as-is
        let paths_match = match (&worktree_canonical, &wt_canonical) {
            (Some(a), Some(b)) => a == b,
            _ => wt.path == worktree_path,
        };

        if paths_match {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Get the path to a worktree
pub fn get_worktree_path(stage_id: &str, repo_root: &Path) -> std::path::PathBuf {
    repo_root.join(".worktrees").join(stage_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_worktree_path() {
        let repo_root = Path::new("/home/user/repo");
        let path = get_worktree_path("stage-1", repo_root);
        assert_eq!(
            path,
            std::path::PathBuf::from("/home/user/repo/.worktrees/stage-1")
        );
    }
}
