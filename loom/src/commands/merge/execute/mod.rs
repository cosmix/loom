//! Main merge execution logic
//!
//! Contains the primary execute function that orchestrates the merge workflow
//! including validation, git operations, conflict recovery, and cleanup.
//!
//! This command serves two purposes:
//! 1. Recovery: Re-spawn conflict resolution session for failed/interrupted merges
//! 2. Manual trigger: Force merge of a completed stage that wasn't auto-merged

mod operations;
mod recovery;
mod session;

#[cfg(test)]
mod tests;

use anyhow::{bail, Result};

use crate::git::branch::branch_name_for_stage;
use crate::git::{
    cleanup_merged_branches, conflict_resolution_instructions, current_branch, ensure_work_symlink,
    merge_stage, remove_worktree, MergeResult,
};

use super::helpers::{
    auto_commit_changes, ensure_work_gitignored, get_uncommitted_files, has_merge_conflicts,
    has_uncommitted_changes, pop_stash, remove_loom_dirs_from_branch, stash_changes,
};
use super::validation::{check_active_session, validate_stage_status};

pub use operations::{mark_stage_merged, worktree_path};
use recovery::{find_active_merge_session, get_current_conflicts};
use session::spawn_merge_resolution_session;

/// Merge worktree branch to main, with recovery support for conflict resolution
///
/// This command serves as both:
/// 1. A recovery command for failed/interrupted merge sessions
/// 2. A manual merge trigger for completed stages
///
/// # Recovery Flow
/// When a previous merge attempt resulted in conflicts and the resolution session
/// was interrupted, this command will:
/// - Detect existing merge conflicts in the main repository
/// - Spawn a new Claude Code session to resolve them
/// - Report if a merge session is already running
///
/// # Manual Merge Flow
/// For stages that completed but weren't auto-merged:
/// - Validates stage status (Completed/Verified required)
/// - Checks for active sessions
/// - Performs the merge with auto-commit of uncommitted changes
///
/// # Arguments
/// * `stage_id` - The ID of the stage to merge
/// * `force` - If true, skip safety checks (DANGEROUS)
pub fn execute(stage_id: String, force: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;
    let work_dir = repo_root.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    // Check worktree existence
    let worktree_path = repo_root.join(".worktrees").join(&stage_id);
    let worktree_exists = worktree_path.exists();

    // RECOVERY PATH: Check if we're in a merge conflict state
    if has_merge_conflicts(&repo_root)? {
        println!("Detected merge conflict state in main repository.");
        println!("Checking for existing merge resolution session...");

        // Check if a merge session is already running
        if let Some(session_name) = find_active_merge_session(&stage_id, &work_dir)? {
            println!("\nMerge resolution session is already running: {session_name}");
            return Ok(());
        }

        // Get current conflicts and spawn a new resolution session
        let conflicts = get_current_conflicts(&repo_root)?;
        println!("\nConflicting files:");
        for file in &conflicts {
            println!("  - {file}");
        }

        println!("\nSpawning merge resolution session...");
        let session_id =
            spawn_merge_resolution_session(&stage_id, &conflicts, &repo_root, &work_dir)?;
        println!("Started merge resolution session: {session_id}");
        println!("\nThe session will help resolve conflicts. Once resolved,");
        println!("run 'loom worktree remove {stage_id}' to clean up worktree and branch.");
        return Ok(());
    }

    // Check if worktree doesn't exist - might already be merged
    if !worktree_exists {
        // Check if branch still exists
        let branch_name = branch_name_for_stage(&stage_id);
        let branch_exists = std::process::Command::new("git")
            .args(["rev-parse", "--verify", &branch_name])
            .current_dir(&repo_root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !branch_exists {
            println!("Stage '{stage_id}' appears to be already merged.");
            println!("  - Worktree not found at {}", worktree_path.display());
            println!("  - Branch '{branch_name}' does not exist");
            mark_stage_merged(&stage_id, &work_dir)?;
            return Ok(());
        }

        bail!(
            "Worktree for stage '{stage_id}' not found at {}\n\
             Branch '{branch_name}' exists but worktree is missing.\n\
             \n\
             If the merge was already completed (conflicts resolved), run:\n\
                loom worktree remove {stage_id}\n\
             \n\
             Otherwise, to recreate the worktree:\n\
             1. git worktree add .worktrees/{stage_id} {branch_name}\n\
             2. loom merge {stage_id}",
            worktree_path.display(),
            stage_id = stage_id,
            branch_name = branch_name
        );
    }

    // NORMAL MERGE PATH: Perform the merge
    println!("Merging stage: {stage_id}");

    // Safety check 1: Validate stage status
    validate_stage_status(&stage_id, &work_dir, force)?;

    // Safety check 2: Check for active sessions
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

    // Determine target branch - merge to the current branch of the main repo
    let target_branch =
        current_branch(&repo_root).with_context(|| "Failed to get current branch")?;
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

    // Helper to handle post-merge cleanup with stash safety
    let cleanup_result = match merge_result {
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
            Ok(())
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
            Ok(())
        }
        MergeResult::AlreadyUpToDate => {
            println!("Branch is already up to date with {target_branch}.");
            println!("Removing worktree anyway...");

            remove_worktree(&stage_id, &repo_root, true)?;
            println!("Worktree removed: {}", worktree_path.display());

            // Clean up merged branch
            let cleaned = cleanup_merged_branches(&target_branch, &repo_root)?;
            if !cleaned.is_empty() {
                println!("Cleaned up branches: {}", cleaned.join(", "));
            }

            mark_stage_merged(&stage_id, &work_dir)?;
            Ok(())
        }
        MergeResult::Conflict { conflicting_files } => {
            // Restore .work symlink so worktree remains functional for conflict resolution
            if let Err(restore_err) = ensure_work_symlink(&worktree_path, &repo_root) {
                eprintln!("Warning: Failed to restore .work symlink: {restore_err}");
            }
            // Restore stash before handling conflict
            if main_repo_stashed {
                eprintln!("\nRestoring stashed changes...");
                pop_stash(&repo_root)?;
            }

            println!("\nMerge conflict detected. Spawning resolution session...");

            // Spawn a merge resolution session
            let session_id = spawn_merge_resolution_session(
                &stage_id,
                &conflicting_files,
                &repo_root,
                &work_dir,
            )?;

            let instructions =
                conflict_resolution_instructions(&stage_id, &target_branch, &conflicting_files);
            println!("\n{instructions}");
            println!("\nStarted merge resolution session: {session_id}");
            println!("The session will help resolve conflicts.");
            println!("Once resolved, run 'loom worktree remove {stage_id}' to clean up.");

            return Ok(());
        }
    };

    // If cleanup failed, pop stash before propagating error
    if let Err(e) = cleanup_result {
        if main_repo_stashed {
            eprintln!("\nPost-merge cleanup failed, restoring stashed changes...");
            pop_stash(&repo_root)?;
        }
        return Err(e);
    }

    // Restore stashed changes after successful merge
    if main_repo_stashed {
        println!("\nRestoring stashed changes...");
        pop_stash(&repo_root)?;
    }

    Ok(())
}

use anyhow::Context;
