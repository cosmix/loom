//! Main merge execution logic
//!
//! Contains the primary execute function that orchestrates the merge workflow
//! including validation, git operations, and cleanup.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use crate::git::{
    cleanup_merged_branches, conflict_resolution_instructions, default_branch, ensure_work_symlink,
    merge_stage, remove_worktree, MergeResult,
};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, transition_stage};

use super::helpers::{
    auto_commit_changes, ensure_work_gitignored, get_uncommitted_files, has_uncommitted_changes,
    pop_stash, remove_loom_dirs_from_branch, stash_changes,
};
use super::validation::{check_active_session, validate_stage_status};

use crate::fs::stage_files::find_stage_file;

/// Update stage status to Verified after successful merge
fn mark_stage_merged(stage_id: &str, work_dir: &std::path::Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");

    // Only update if stage file exists
    if find_stage_file(&stages_dir, stage_id)?.is_none() {
        // Stage file doesn't exist (might be a worktree without loom tracking)
        return Ok(());
    }

    // Transition to Verified status (if not already)
    let stage = load_stage(stage_id, work_dir)?;
    if stage.status != StageStatus::Verified {
        transition_stage(stage_id, StageStatus::Verified, work_dir)
            .with_context(|| format!("Failed to update stage status for: {stage_id}"))?;
        println!("Updated stage status to Verified");
    }

    Ok(())
}

/// Merge worktree branch to main, remove worktree on success
///
/// # Safety Checks (unless --force is used)
/// - Stage must be in Completed or Verified status
/// - No active tmux sessions for this stage
///
/// # Arguments
/// * `stage_id` - The ID of the stage to merge
/// * `force` - If true, skip safety checks (DANGEROUS)
pub fn execute(stage_id: String, force: bool) -> Result<()> {
    println!("Merging stage: {stage_id}");

    let repo_root = std::env::current_dir()?;
    let work_dir = repo_root.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    // Check worktree exists
    let worktree_path = repo_root.join(".worktrees").join(&stage_id);
    if !worktree_path.exists() {
        bail!(
            "Worktree for stage '{stage_id}' not found at {}",
            worktree_path.display()
        );
    }

    // Safety check 1: Validate stage status
    validate_stage_status(&stage_id, &work_dir, force)?;

    // Safety check 2: Check for active sessions (both tmux and native)
    check_active_session(&stage_id, &work_dir, force)?;

    println!("Worktree path: {}", worktree_path.display());
    println!("Branch to merge: loom/{stage_id}");

    // Check for uncommitted changes and auto-commit them
    if has_uncommitted_changes(&worktree_path)? {
        let files = get_uncommitted_files(&worktree_path)?;
        println!("\nFound {} uncommitted file(s) in worktree:", files.len());
        for file in &files {
            println!("  - {file}");
        }
        println!("\nAuto-committing changes before merge...");
        auto_commit_changes(&stage_id, &worktree_path)?;
        println!("Changes committed.");
    }

    // Remove .work symlink from worktree before merge
    // This prevents "would lose untracked files" errors during merge
    let work_symlink = worktree_path.join(".work");
    if work_symlink.is_symlink() {
        std::fs::remove_file(&work_symlink).with_context(|| {
            format!(
                "Failed to remove .work symlink from worktree: {}",
                work_symlink.display()
            )
        })?;
    }

    // Determine target branch
    let target_branch = default_branch(&repo_root)
        .with_context(|| "Failed to detect default branch (main/master)")?;
    println!("Target branch: {target_branch}");

    // Ensure .work is in .gitignore to prevent merge conflicts
    // Git can fail with "would lose untracked files" if .work isn't ignored
    ensure_work_gitignored(&repo_root)?;

    // Remove .work and .worktrees from the branch if accidentally committed
    // This can happen if the gitignore wasn't set up before worktree creation
    remove_loom_dirs_from_branch(&stage_id, &worktree_path)?;

    // Auto-stash uncommitted changes in main repo (required for checkout)
    // Uses -u flag to include untracked files
    let main_repo_stashed = if has_uncommitted_changes(&repo_root)? {
        let files = get_uncommitted_files(&repo_root)?;
        println!("\nMain repository has {} uncommitted file(s):", files.len());
        for file in files.iter().take(5) {
            println!("  - {file}");
        }
        if files.len() > 5 {
            println!("  ... and {} more", files.len() - 5);
        }
        println!("\nAuto-stashing changes (will restore after merge)...");
        stash_changes(&repo_root, "loom: auto-stash before merge")?;
        println!("Changes stashed.");
        true
    } else {
        false
    };

    // Perform the merge (restore stash on error)
    println!("\nMerging loom/{stage_id} into {target_branch}...");
    let merge_result = match merge_stage(&stage_id, &target_branch, &repo_root) {
        Ok(result) => result,
        Err(e) => {
            // Restore .work symlink so worktree remains functional
            if let Err(restore_err) = ensure_work_symlink(&worktree_path, &repo_root) {
                eprintln!("Warning: Failed to restore .work symlink: {restore_err}");
            }
            // Restore stash before returning error
            if main_repo_stashed {
                eprintln!("\nMerge failed, restoring stashed changes...");
                pop_stash(&repo_root)?;
            }
            return Err(e);
        }
    };

    match merge_result {
        MergeResult::Success {
            files_changed,
            insertions,
            deletions,
        } => {
            println!("Merge successful!");
            println!("  {files_changed} files changed, +{insertions} -{deletions}");

            // Remove worktree (force=true since work is safely merged)
            println!("\nRemoving worktree...");
            remove_worktree(&stage_id, &repo_root, true)?;
            println!("Worktree removed: {}", worktree_path.display());

            // Clean up merged branch
            let cleaned = cleanup_merged_branches(&target_branch, &repo_root)?;
            if !cleaned.is_empty() {
                println!("Cleaned up branches: {}", cleaned.join(", "));
            }

            // Update stage status
            mark_stage_merged(&stage_id, &work_dir)?;
            println!("\nStage '{stage_id}' merged successfully!");
        }
        MergeResult::FastForward => {
            println!("Fast-forward merge completed!");

            remove_worktree(&stage_id, &repo_root, true)?;
            println!("Worktree removed: {}", worktree_path.display());

            let cleaned = cleanup_merged_branches(&target_branch, &repo_root)?;
            if !cleaned.is_empty() {
                println!("Cleaned up branches: {}", cleaned.join(", "));
            }

            mark_stage_merged(&stage_id, &work_dir)?;
            println!("\nStage '{stage_id}' merged successfully!");
        }
        MergeResult::AlreadyUpToDate => {
            println!("Branch is already up to date with {target_branch}.");
            println!("Removing worktree anyway...");

            remove_worktree(&stage_id, &repo_root, true)?;
            println!("Worktree removed: {}", worktree_path.display());

            mark_stage_merged(&stage_id, &work_dir)?;
        }
        MergeResult::Conflict { conflicting_files } => {
            // Restore .work symlink so worktree remains functional for conflict resolution
            if let Err(restore_err) = ensure_work_symlink(&worktree_path, &repo_root) {
                eprintln!("Warning: Failed to restore .work symlink: {restore_err}");
            }
            // Restore stash before showing conflict instructions
            if main_repo_stashed {
                eprintln!("\nRestoring stashed changes...");
                pop_stash(&repo_root)?;
            }
            let instructions =
                conflict_resolution_instructions(&stage_id, &target_branch, &conflicting_files);
            eprintln!("\n{instructions}");
            bail!(
                "Merge conflicts detected. Resolve manually, then run 'loom merge {stage_id}' again."
            );
        }
    }

    // Restore stashed changes after successful merge
    if main_repo_stashed {
        println!("\nRestoring stashed changes...");
        pop_stash(&repo_root)?;
    }

    Ok(())
}

/// Get the worktree path for a stage
pub fn worktree_path(stage_id: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".worktrees")
        .join(stage_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_path() {
        let path = worktree_path("stage-1");
        assert!(path.to_string_lossy().contains(".worktrees"));
        assert!(path.to_string_lossy().contains("stage-1"));
    }
}
