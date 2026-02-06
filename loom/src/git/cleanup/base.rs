//! Base branch cleanup operations
//!
//! This module provides functions for cleaning up temporary base branches
//! (`loom/_base/{stage-id}`) that are created when a stage has multiple
//! dependencies that need to be merged together before work can begin.

use anyhow::Result;
use std::path::Path;

use crate::git::branch::delete_branch;
use crate::git::runner::{run_git_bool, run_git_checked};

/// Clean up the base branch for a stage
///
/// Base branches are created when a stage has multiple dependencies that
/// need to be merged together. They follow the pattern `loom/_base/{stage-id}`.
///
/// # Arguments
/// * `stage_id` - The stage ID whose base branch to delete
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// `true` if the branch was deleted, `false` if it didn't exist
pub fn cleanup_base_branch(stage_id: &str, repo_root: &Path) -> Result<bool> {
    let branch_name = format!("loom/_base/{stage_id}");

    // Check if branch exists first
    let ref_path = format!("refs/heads/{branch_name}");
    if !run_git_bool(&["rev-parse", "--verify", &ref_path], repo_root) {
        // Branch doesn't exist
        return Ok(false);
    }

    // Delete the branch (force deletion since base branches are temporary)
    delete_branch(&branch_name, true, repo_root)?;
    Ok(true)
}

/// Clean up all base branches in the repository
///
/// This deletes all branches matching the pattern `loom/_base/*`.
/// Useful for cleaning up after all stages are complete or when
/// resetting the orchestration state.
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// A vector of branch names that were deleted
pub fn cleanup_all_base_branches(repo_root: &Path) -> Result<Vec<String>> {
    // List all branches matching the pattern
    let stdout = run_git_checked(&["branch", "--list", "loom/_base/*"], repo_root)?;

    let branches: Vec<String> = stdout
        .lines()
        .map(|s| s.trim().trim_start_matches('*').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut deleted = Vec::new();
    for branch in branches {
        if delete_branch(&branch, true, repo_root).is_ok() {
            deleted.push(branch);
        }
    }

    Ok(deleted)
}

/// Check if a base branch exists for a stage
///
/// # Arguments
/// * `stage_id` - The stage ID to check
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// `true` if the base branch exists, `false` otherwise
pub fn base_branch_exists(stage_id: &str, repo_root: &Path) -> Result<bool> {
    let branch_name = format!("loom/_base/{stage_id}");
    let ref_path = format!("refs/heads/{branch_name}");
    Ok(run_git_bool(&["rev-parse", "--verify", &ref_path], repo_root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Create initial commit
        let test_file = temp_dir.path().join("README.md");
        fs::write(&test_file, "# Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        temp_dir
    }

    #[test]
    fn test_cleanup_base_branch_nonexistent() {
        let temp_dir = setup_git_repo();
        let result = cleanup_base_branch("nonexistent", temp_dir.path());
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_cleanup_base_branch_exists() {
        let temp_dir = setup_git_repo();

        // Create a base branch
        Command::new("git")
            .args(["branch", "loom/_base/stage-1"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let result = cleanup_base_branch("stage-1", temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Verify branch is deleted
        let exists = base_branch_exists("stage-1", temp_dir.path()).unwrap();
        assert!(!exists);
    }

    #[test]
    fn test_cleanup_all_base_branches_empty() {
        let temp_dir = setup_git_repo();
        let result = cleanup_all_base_branches(temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_cleanup_all_base_branches_multiple() {
        let temp_dir = setup_git_repo();

        // Create multiple base branches
        Command::new("git")
            .args(["branch", "loom/_base/stage-1"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "loom/_base/stage-2"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "loom/_base/stage-3"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let result = cleanup_all_base_branches(temp_dir.path());
        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert_eq!(deleted.len(), 3);

        // Verify all branches are deleted
        assert!(!base_branch_exists("stage-1", temp_dir.path()).unwrap());
        assert!(!base_branch_exists("stage-2", temp_dir.path()).unwrap());
        assert!(!base_branch_exists("stage-3", temp_dir.path()).unwrap());
    }

    #[test]
    fn test_base_branch_exists() {
        let temp_dir = setup_git_repo();

        // Should not exist initially
        assert!(!base_branch_exists("stage-1", temp_dir.path()).unwrap());

        // Create the branch
        Command::new("git")
            .args(["branch", "loom/_base/stage-1"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Should exist now
        assert!(base_branch_exists("stage-1", temp_dir.path()).unwrap());
    }
}
