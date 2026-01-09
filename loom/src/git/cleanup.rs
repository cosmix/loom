//! Cleanup utilities for successful merge operations
//!
//! This module provides reusable functions for cleaning up resources after
//! a successful merge. It consolidates cleanup logic that was previously
//! duplicated across multiple commands (verify, stage complete, clean).
//!
//! ## Cleanup Phases
//!
//! A successful merge cleanup involves several phases:
//! 1. Worktree removal - Remove the isolated git worktree
//! 2. Branch deletion - Delete the loom/{stage-id} branch
//! 3. Git pruning - Clean up any stale worktree references
//!
//! ## Usage
//!
//! ```rust,ignore
//! use loom::git::cleanup::{CleanupConfig, cleanup_after_merge};
//!
//! let config = CleanupConfig::default();
//! cleanup_after_merge("stage-1", repo_root, &config)?;
//! ```

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use super::branch::delete_branch;
use super::worktree::remove_worktree;

/// Configuration for cleanup operations
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// Force removal even if worktree has uncommitted changes
    pub force_worktree_removal: bool,
    /// Force branch deletion even if not fully merged
    pub force_branch_deletion: bool,
    /// Run git worktree prune after cleanup
    pub prune_worktrees: bool,
    /// Print progress messages
    pub verbose: bool,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            force_worktree_removal: true,
            force_branch_deletion: false,
            prune_worktrees: true,
            verbose: true,
        }
    }
}

impl CleanupConfig {
    /// Create a quiet config (no verbose output)
    pub fn quiet() -> Self {
        Self {
            verbose: false,
            ..Self::default()
        }
    }

    /// Create a config for forced cleanup (for use by loom clean command)
    pub fn forced() -> Self {
        Self {
            force_worktree_removal: true,
            force_branch_deletion: true,
            prune_worktrees: true,
            verbose: true,
        }
    }
}

/// Result of a cleanup operation
#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    /// Whether the worktree was successfully removed
    pub worktree_removed: bool,
    /// Whether the branch was successfully deleted
    pub branch_deleted: bool,
    /// Errors that occurred (non-fatal)
    pub warnings: Vec<String>,
}

impl CleanupResult {
    /// Check if cleanup was fully successful (no warnings)
    pub fn is_complete(&self) -> bool {
        self.worktree_removed && self.branch_deleted && self.warnings.is_empty()
    }

    /// Check if cleanup made any progress
    pub fn any_cleanup_done(&self) -> bool {
        self.worktree_removed || self.branch_deleted
    }
}

/// Perform full cleanup after a successful merge
///
/// This function removes all resources associated with a stage after its
/// branch has been successfully merged. It's safe to call even if some
/// resources have already been cleaned up.
///
/// # Arguments
/// * `stage_id` - The stage ID to clean up
/// * `repo_root` - Path to the repository root
/// * `config` - Cleanup configuration options
///
/// # Returns
/// A `CleanupResult` describing what was cleaned up
pub fn cleanup_after_merge(
    stage_id: &str,
    repo_root: &Path,
    config: &CleanupConfig,
) -> Result<CleanupResult> {
    let mut result = CleanupResult::default();
    let branch_name = format!("loom/{stage_id}");

    // Phase 1: Remove the worktree
    if config.verbose {
        println!("Cleaning up worktree for stage '{stage_id}'...");
    }

    match cleanup_worktree(stage_id, repo_root, config.force_worktree_removal) {
        Ok(removed) => {
            result.worktree_removed = removed;
            if config.verbose && removed {
                println!("  Removed worktree: .worktrees/{stage_id}");
            }
        }
        Err(e) => {
            let msg = format!("Failed to remove worktree: {e}");
            if config.verbose {
                eprintln!("  Warning: {msg}");
            }
            result.warnings.push(msg);
        }
    }

    // Phase 2: Delete the branch
    if config.verbose {
        println!("Cleaning up branch '{branch_name}'...");
    }

    match cleanup_branch(stage_id, repo_root, config.force_branch_deletion) {
        Ok(deleted) => {
            result.branch_deleted = deleted;
            if config.verbose && deleted {
                println!("  Deleted branch: {branch_name}");
            }
        }
        Err(e) => {
            let msg = format!("Failed to delete branch: {e}");
            if config.verbose {
                eprintln!("  Warning: {msg}");
            }
            result.warnings.push(msg);
        }
    }

    // Phase 3: Prune stale worktree references
    if config.prune_worktrees {
        if let Err(e) = prune_worktrees(repo_root) {
            let msg = format!("Failed to prune worktrees: {e}");
            if config.verbose {
                eprintln!("  Warning: {msg}");
            }
            result.warnings.push(msg);
        }
    }

    Ok(result)
}

/// Clean up a single worktree for a stage
///
/// # Arguments
/// * `stage_id` - The stage ID whose worktree to remove
/// * `repo_root` - Path to the repository root
/// * `force` - Force removal even with uncommitted changes
///
/// # Returns
/// `true` if the worktree was removed, `false` if it didn't exist
pub fn cleanup_worktree(stage_id: &str, repo_root: &Path, force: bool) -> Result<bool> {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);

    if !worktree_path.exists() {
        return Ok(false);
    }

    // Remove symlinks first to avoid issues with git worktree remove
    remove_worktree_symlinks(&worktree_path)?;

    // Try to remove via git worktree
    match remove_worktree(stage_id, repo_root, force) {
        Ok(()) => Ok(true),
        Err(e) => {
            // If git worktree remove fails, try manual cleanup
            std::fs::remove_dir_all(&worktree_path).with_context(|| {
                format!(
                    "Failed to manually remove worktree at {} after git error: {}",
                    worktree_path.display(),
                    e
                )
            })?;
            Ok(true)
        }
    }
}

/// Remove symlinks from a worktree directory before removal
///
/// Git worktree remove can have issues with symlinks. This function
/// removes known symlinks (.work, .claude) first.
fn remove_worktree_symlinks(worktree_path: &Path) -> Result<()> {
    // Remove .work symlink
    let work_link = worktree_path.join(".work");
    if work_link.exists() || work_link.is_symlink() {
        std::fs::remove_file(&work_link).ok();
    }

    // Remove .claude directory/symlinks
    let claude_dir = worktree_path.join(".claude");
    if claude_dir.is_symlink() {
        std::fs::remove_file(&claude_dir).ok();
    } else if claude_dir.exists() {
        // It's a real directory with symlinks inside
        let claude_md = claude_dir.join("CLAUDE.md");
        if claude_md.is_symlink() {
            std::fs::remove_file(&claude_md).ok();
        }
        let settings = claude_dir.join("settings.local.json");
        if settings.is_symlink() {
            std::fs::remove_file(&settings).ok();
        }
        // Now remove the directory
        std::fs::remove_dir_all(&claude_dir).ok();
    }

    Ok(())
}

/// Clean up the branch for a stage
///
/// # Arguments
/// * `stage_id` - The stage ID whose branch to delete
/// * `repo_root` - Path to the repository root
/// * `force` - Force deletion even if not fully merged
///
/// # Returns
/// `true` if the branch was deleted, `false` if it didn't exist
pub fn cleanup_branch(stage_id: &str, repo_root: &Path, force: bool) -> Result<bool> {
    let branch_name = format!("loom/{stage_id}");

    // Check if branch exists first
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch_name}")])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to check branch existence")?;

    if !output.status.success() {
        // Branch doesn't exist
        return Ok(false);
    }

    // Delete the branch
    delete_branch(&branch_name, force, repo_root)?;
    Ok(true)
}

/// Prune stale worktree references
///
/// Runs `git worktree prune` to clean up any stale worktree metadata.
pub fn prune_worktrees(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to run git worktree prune")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree prune failed: {stderr}");
    }

    Ok(())
}

/// Clean up multiple stages at once
///
/// # Arguments
/// * `stage_ids` - List of stage IDs to clean up
/// * `repo_root` - Path to the repository root
/// * `config` - Cleanup configuration options
///
/// # Returns
/// Map of stage_id to CleanupResult
pub fn cleanup_multiple_stages(
    stage_ids: &[&str],
    repo_root: &Path,
    config: &CleanupConfig,
) -> Vec<(String, CleanupResult)> {
    let mut results = Vec::with_capacity(stage_ids.len());

    for stage_id in stage_ids {
        let result = cleanup_after_merge(stage_id, repo_root, config)
            .unwrap_or_else(|e| CleanupResult {
                worktree_removed: false,
                branch_deleted: false,
                warnings: vec![e.to_string()],
            });
        results.push(((*stage_id).to_string(), result));
    }

    // Final prune after all cleanups
    if config.prune_worktrees {
        let _ = prune_worktrees(repo_root);
    }

    results
}

/// Check if a stage has resources that need cleanup
///
/// Returns true if the stage has a worktree or branch that exists.
pub fn needs_cleanup(stage_id: &str, repo_root: &Path) -> bool {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let branch_name = format!("loom/{stage_id}");

    // Check worktree exists
    if worktree_path.exists() {
        return true;
    }

    // Check branch exists
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch_name}")])
        .current_dir(repo_root)
        .output();

    matches!(output, Ok(o) if o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
    fn test_cleanup_config_default() {
        let config = CleanupConfig::default();
        assert!(config.force_worktree_removal);
        assert!(!config.force_branch_deletion);
        assert!(config.prune_worktrees);
        assert!(config.verbose);
    }

    #[test]
    fn test_cleanup_config_quiet() {
        let config = CleanupConfig::quiet();
        assert!(!config.verbose);
    }

    #[test]
    fn test_cleanup_config_forced() {
        let config = CleanupConfig::forced();
        assert!(config.force_worktree_removal);
        assert!(config.force_branch_deletion);
    }

    #[test]
    fn test_cleanup_result_is_complete() {
        let mut result = CleanupResult::default();
        assert!(!result.is_complete());

        result.worktree_removed = true;
        result.branch_deleted = true;
        assert!(result.is_complete());

        result.warnings.push("warning".to_string());
        assert!(!result.is_complete());
    }

    #[test]
    fn test_cleanup_result_any_cleanup_done() {
        let mut result = CleanupResult::default();
        assert!(!result.any_cleanup_done());

        result.worktree_removed = true;
        assert!(result.any_cleanup_done());
    }

    #[test]
    fn test_cleanup_worktree_nonexistent() {
        let temp_dir = setup_git_repo();
        let result = cleanup_worktree("nonexistent", temp_dir.path(), false);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_cleanup_branch_nonexistent() {
        let temp_dir = setup_git_repo();
        let result = cleanup_branch("nonexistent", temp_dir.path(), false);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_needs_cleanup_no_resources() {
        let temp_dir = setup_git_repo();
        assert!(!needs_cleanup("stage-1", temp_dir.path()));
    }

    #[test]
    fn test_needs_cleanup_with_worktree_dir() {
        let temp_dir = setup_git_repo();
        let worktree_path = temp_dir.path().join(".worktrees").join("stage-1");
        fs::create_dir_all(&worktree_path).unwrap();

        assert!(needs_cleanup("stage-1", temp_dir.path()));
    }

    #[test]
    fn test_needs_cleanup_with_branch() {
        let temp_dir = setup_git_repo();

        // Create a branch
        Command::new("git")
            .args(["branch", "loom/stage-1"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(needs_cleanup("stage-1", temp_dir.path()));
    }

    #[test]
    fn test_prune_worktrees() {
        let temp_dir = setup_git_repo();
        let result = prune_worktrees(temp_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_cleanup_after_merge_nothing_to_clean() {
        let temp_dir = setup_git_repo();
        let config = CleanupConfig::quiet();

        let result = cleanup_after_merge("nonexistent", temp_dir.path(), &config);
        assert!(result.is_ok());

        let cleanup_result = result.unwrap();
        assert!(!cleanup_result.worktree_removed);
        assert!(!cleanup_result.branch_deleted);
    }

    #[test]
    fn test_cleanup_multiple_stages_empty() {
        let temp_dir = setup_git_repo();
        let config = CleanupConfig::quiet();

        let results = cleanup_multiple_stages(&[], temp_dir.path(), &config);
        assert!(results.is_empty());
    }

    #[test]
    fn test_remove_worktree_symlinks() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path().join("worktree");
        fs::create_dir_all(&worktree_path).unwrap();

        // Create .claude directory with symlinks (simulated as files for testing)
        let claude_dir = worktree_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("CLAUDE.md"), "test").unwrap();
        fs::write(claude_dir.join("settings.local.json"), "{}").unwrap();

        let result = remove_worktree_symlinks(&worktree_path);
        assert!(result.is_ok());
    }
}
