//! Progressive merge integration for stage completion
//!
//! This module handles the git merge operations that occur when a stage
//! completes successfully with passing acceptance criteria.

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::{cleanup_after_merge, CleanupConfig};
use crate::git::get_branch_head;
use crate::models::stage::Stage;
use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};
use crate::verify::transitions::save_stage;

/// Result of attempting to merge a completed stage
pub enum MergeOutcome {
    /// Merge succeeded - stage can be marked completed
    Success,
    /// Merge conflict - stage should be marked MergeConflict
    Conflict,
    /// Merge blocked - stage should be marked MergeBlocked
    Blocked,
}

/// Attempt to progressively merge a completed stage into the merge point.
///
/// This function handles the git merge operations and updates the stage's
/// merge-related fields (merged, completed_commit).
///
/// # Returns
/// - `MergeOutcome::Success` if merge succeeded (stage should be marked Completed)
/// - `MergeOutcome::Conflict` if merge conflict (stage already marked MergeConflict)
/// - `MergeOutcome::Blocked` if merge failed (stage already marked MergeBlocked)
pub fn attempt_progressive_merge(
    stage: &mut Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<MergeOutcome> {
    let merge_point = get_merge_point(work_dir)?;

    // Capture the completed commit SHA before merge (the HEAD of the stage branch)
    let branch_name = branch_name_for_stage(&stage.id);
    let completed_commit = get_branch_head(&branch_name, repo_root).ok();

    println!("Attempting progressive merge into '{merge_point}'...");
    match merge_completed_stage(stage, repo_root, &merge_point) {
        Ok(ProgressiveMergeResult::Success { files_changed }) => {
            println!("  ✓ Merged {files_changed} file(s) into '{merge_point}'");
            stage.completed_commit = completed_commit;
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::FastForward) => {
            println!("  ✓ Fast-forward merge into '{merge_point}'");
            stage.completed_commit = completed_commit;
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::AlreadyMerged) => {
            println!("  ✓ Already up to date with '{merge_point}'");
            stage.completed_commit = completed_commit;
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::NoBranch) => {
            println!("  → No branch to merge (already cleaned up)");
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::Conflict { conflicting_files }) => {
            println!("  ✗ Merge conflict detected!");
            println!("    Conflicting files:");
            for file in &conflicting_files {
                println!("      - {file}");
            }
            println!();
            println!("    Stage transitioning to MergeConflict status.");
            stage.try_mark_merge_conflict()?;
            save_stage(stage, work_dir)?;

            // Try to auto-spawn a merge resolver session
            use super::merge_resolver::MergeResolverResult;
            match super::merge_resolver::spawn_merge_resolver(
                stage,
                &conflicting_files,
                &merge_point,
                repo_root,
                work_dir,
            ) {
                Ok(MergeResolverResult::DaemonManaged) => {
                    println!(
                        "    Daemon is running - merge resolution will be handled automatically."
                    );
                }
                Ok(MergeResolverResult::Spawned(id)) => {
                    println!("    Spawned merge resolver session: {id}");
                }
                Err(e) => {
                    eprintln!("    Failed to spawn merge resolver: {e}");
                    println!(
                        "    Resolve conflicts manually and run: loom stage merge-complete {}",
                        stage.id
                    );
                }
            }

            Ok(MergeOutcome::Conflict)
        }
        Err(e) => {
            eprintln!("Progressive merge failed: {e}");
            stage.try_mark_merge_blocked()?;
            save_stage(stage, work_dir)?;
            eprintln!("Stage '{}' marked as MergeBlocked", stage.id);
            eprintln!("  Fix the issue and run: loom stage retry {}", stage.id);
            Ok(MergeOutcome::Blocked)
        }
    }
}

/// Complete a stage with merge, triggering dependents on success.
///
/// This is the standard completion path for stages after acceptance criteria pass.
/// It attempts progressive merge and marks the stage as completed.
pub fn complete_with_merge(stage: &mut Stage, repo_root: &Path, work_dir: &Path) -> Result<bool> {
    match attempt_progressive_merge(stage, repo_root, work_dir)? {
        MergeOutcome::Success => {
            // Mark stage as completed - only after merge succeeds
            stage.try_complete(None)?;
            save_stage(stage, work_dir)?;

            println!("Stage '{}' completed!", stage.id);

            // Trigger dependent stages
            let triggered = crate::verify::transitions::trigger_dependents(&stage.id, work_dir)
                .context("Failed to trigger dependent stages")?;

            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  → {dep_id}");
                }
            }

            // Clean up worktree and branch after successful merge
            let cleanup_config = CleanupConfig {
                verbose: true,
                force_worktree_removal: false,
                force_branch_deletion: false,
                prune_worktrees: true,
            };

            match cleanup_after_merge(&stage.id, repo_root, &cleanup_config) {
                Ok(result) => {
                    if result.worktree_removed {
                        println!("  Removed worktree: .worktrees/{}", stage.id);
                    }
                    if result.branch_deleted {
                        println!("  Deleted branch: {}", branch_name_for_stage(&stage.id));
                    }
                    if !result.warnings.is_empty() {
                        for warning in &result.warnings {
                            eprintln!("  Warning: {warning}");
                        }
                    }
                }
                Err(e) => {
                    // Cleanup failure is not fatal - stage is already completed
                    eprintln!("  Warning: Failed to clean up stage resources: {e}");
                    eprintln!(
                        "  You can manually clean up with: loom worktree remove {}",
                        stage.id
                    );
                }
            }

            Ok(true)
        }
        MergeOutcome::Conflict | MergeOutcome::Blocked => {
            // Stage already saved in conflict/blocked state
            Ok(false)
        }
    }
}
