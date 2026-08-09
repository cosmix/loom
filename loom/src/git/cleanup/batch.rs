//! Batch cleanup operations

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use super::base::cleanup_base_branch;
use super::branch::cleanup_branch;
use super::config::{CleanupConfig, CleanupResult};
use super::worktree::cleanup_worktree;
use crate::git::branch::branch_name_for_stage;

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
    let branch_name = branch_name_for_stage(stage_id);
    let worktree_removed = cleanup_worktree(stage_id, repo_root, config.force_worktree_removal)
        .with_context(|| format!("Failed to remove worktree for stage '{stage_id}'"))?;
    let branch_deleted = cleanup_branch(stage_id, repo_root, config.force_branch_deletion)
        .with_context(|| format!("Failed to delete branch '{branch_name}'"))?;
    let base_branch_deleted = cleanup_base_branch(stage_id, repo_root)
        .with_context(|| format!("Failed to delete base branch for stage '{stage_id}'"))?;
    if config.prune_worktrees {
        prune_worktrees(repo_root).context("Failed to prune worktree metadata")?;
    }

    let result = CleanupResult {
        worktree_removed,
        branch_deleted,
        base_branch_deleted,
        warnings: Vec::new(),
    };
    if config.verbose {
        report_cleanup(stage_id, &branch_name, &result);
    }
    Ok(result)
}

fn report_cleanup(stage_id: &str, branch_name: &str, result: &CleanupResult) {
    if result.worktree_removed {
        println!("  Removed worktree: .worktrees/{stage_id}");
    }
    if result.branch_deleted {
        println!("  Deleted branch: {branch_name}");
    }
    if result.base_branch_deleted {
        println!("  Deleted base branch: loom/_base/{stage_id}");
    }
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
        let result =
            cleanup_after_merge(stage_id, repo_root, config).unwrap_or_else(|e| CleanupResult {
                worktree_removed: false,
                branch_deleted: false,
                base_branch_deleted: false,
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
    let branch_name = branch_name_for_stage(stage_id);

    // Check worktree exists
    if worktree_path.exists() {
        return true;
    }

    // Check branch exists
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            &format!("refs/heads/{branch_name}"),
        ])
        .current_dir(repo_root)
        .output();

    matches!(output, Ok(o) if o.status.success())
}
