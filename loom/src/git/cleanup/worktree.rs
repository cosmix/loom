//! Worktree cleanup operations

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::worktree::remove_worktree;

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
            // `git worktree remove` (non-force) refuses when the worktree has
            // modified or untracked files. Falling back to `remove_dir_all`
            // unconditionally — as the old code did — made the `force` flag
            // meaningless and destroyed uncommitted work that the caller
            // deliberately asked to preserve (complete_with_merge passes
            // force:false for exactly this reason). Only escalate to a hard
            // directory wipe when force was requested; otherwise propagate
            // git's refusal so the caller sees the real error.
            if force {
                std::fs::remove_dir_all(&worktree_path).with_context(|| {
                    format!(
                        "Failed to manually remove worktree at {} after git error: {}",
                        worktree_path.display(),
                        e
                    )
                })?;
                Ok(true)
            } else {
                Err(e).with_context(|| {
                    format!(
                        "git worktree remove refused for {} and force was not set; \
                         not destroying uncommitted files. Re-run with force to override.",
                        worktree_path.display()
                    )
                })
            }
        }
    }
}

/// Remove symlinks from a worktree directory before removal
///
/// Git worktree remove can have issues with symlinks. This function
/// removes known symlinks (.work, .claude) first.
pub(crate) fn remove_worktree_symlinks(worktree_path: &Path) -> Result<()> {
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
        let settings = claude_dir.join("settings.json");
        if settings.is_symlink() {
            std::fs::remove_file(&settings).ok();
        }
        // Now remove the directory
        std::fs::remove_dir_all(&claude_dir).ok();
    }

    Ok(())
}
