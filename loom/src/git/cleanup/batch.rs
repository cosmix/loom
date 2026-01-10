//! Batch cleanup operations

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use super::branch::cleanup_branch;
use super::config::{CleanupConfig, CleanupResult};
use super::worktree::cleanup_worktree;

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
        .args([
            "rev-parse",
            "--verify",
            &format!("refs/heads/{branch_name}"),
        ])
        .current_dir(repo_root)
        .output();

    matches!(output, Ok(o) if o.status.success())
}
