//! Progressive merge integration for stage completion
//!
//! This module handles the git merge operations that occur when a stage
//! completes successfully with passing acceptance criteria.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::{cleanup_after_merge, CleanupConfig};
use crate::git::get_branch_head;
use crate::models::stage::Stage;
use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};
use crate::verify::transitions::update_stage;

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
/// # Concurrency (A-5)
///
/// The in-memory `stage` handed to `complete_with_merge` was loaded before the
/// (potentially multi-minute) acceptance and verification phases ran, so it is
/// stale relative to concurrent daemon/dispute writes. The git merge itself runs
/// *outside* the stages-dir lock (it holds the separate `MergeLock` and can take
/// time), then the merge-completion fields are re-applied to the **fresh**
/// on-disk stage via `update_stage`. The completion operation owns exactly:
/// `completed_commit`, `merged`, `merge_conflict`, and the status transition; it
/// re-applies only those, so a concurrent writer's unrelated fields
/// (`dispute_count`, `retry_count`, amended `acceptance`, …) survive. The
/// in-memory `stage` is also updated so the caller can drive resolver spawning.
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

    // Capture the completed commit SHA from the stage branch HEAD.
    // Only assign when get_branch_head succeeds — overwriting a previously
    // persisted completed_commit with None would lose the ancestry proof.
    let branch_name = branch_name_for_stage(&stage.id);
    if let Ok(commit) = get_branch_head(&branch_name, repo_root) {
        stage.completed_commit = Some(commit);
    }
    // Snapshot the commit to re-apply onto the fresh on-disk stage below.
    let completed_commit = stage.completed_commit.clone();

    println!("Attempting progressive merge into '{merge_point}'...");
    match merge_completed_stage(stage, repo_root, &merge_point) {
        Ok(ProgressiveMergeResult::Success { files_changed }) => {
            println!("  ✓ Merged {files_changed} file(s) into '{merge_point}'");
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::FastForward) => {
            println!("  ✓ Fast-forward merge into '{merge_point}'");
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::AlreadyMerged) => {
            println!("  ✓ Already up to date with '{merge_point}'");
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::NoBranch) => {
            tracing::error!(
                stage_id = %stage.id,
                "Progressive merge: branch missing — cannot verify merge succeeded"
            );
            stage.try_mark_merge_blocked()?;
            // Re-apply only the merge-block transition onto the fresh on-disk
            // stage (A-5). completed_commit is preserved if it was captured.
            update_stage(&stage.id, work_dir, |s| {
                s.completed_commit = completed_commit.clone();
                s.try_mark_merge_blocked()
            })?;
            Ok(MergeOutcome::Blocked)
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
            // Re-apply only the merge-conflict transition + commit onto the fresh
            // on-disk stage (A-5).
            update_stage(&stage.id, work_dir, |s| {
                s.completed_commit = completed_commit.clone();
                s.try_mark_merge_conflict()
            })?;

            // Try to auto-spawn a merge resolver session. Fresh-conflict path:
            // the test merge in merge_stage was already aborted before
            // returning Conflict, so there is no active MERGE_HEAD here.
            use super::merge_resolver::MergeResolverResult;
            match super::merge_resolver::spawn_merge_resolver(
                stage,
                &conflicting_files,
                &merge_point,
                None,
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
                Ok(MergeResolverResult::AlreadyRunning { session_id }) => {
                    println!("    Merge resolver session '{session_id}' is already running.");
                }
                Err(e) => {
                    eprintln!("    Failed to spawn merge resolver: {e}");
                    println!(
                        "    Resolve conflicts manually and run: loom stage merge {} --resolved",
                        stage.id
                    );
                }
            }

            Ok(MergeOutcome::Conflict)
        }
        Err(e) => {
            eprintln!("Progressive merge failed: {e}");
            stage.try_mark_merge_blocked()?;
            // Re-apply only the merge-block transition onto the fresh on-disk
            // stage (A-5).
            update_stage(&stage.id, work_dir, |s| {
                s.completed_commit = completed_commit.clone();
                s.try_mark_merge_blocked()
            })?;
            eprintln!("Stage '{}' marked as MergeBlocked", stage.id);
            eprintln!("  Fix the issue and run: loom stage retry {}", stage.id);
            Ok(MergeOutcome::Blocked)
        }
    }
}

/// Whether worktree cleanup for `stage_id` must be deferred rather than run
/// now.
///
/// Removing `repo_root/.worktrees/<stage_id>` while `cwd` is inside it
/// deletes the current process's (and its parent Claude session's) live
/// working directory, which breaks any hooks the session fires afterward —
/// they spawn a shell with a cwd that no longer exists. When that's the
/// case, the caller must skip immediate cleanup and leave it for the
/// orchestrator (which cleans up after killing the session).
pub(super) fn should_defer_cleanup(cwd: &Path, repo_root: &Path, stage_id: &str) -> bool {
    let expected = repo_root.join(".worktrees").join(stage_id);
    let expected = match expected.canonicalize() {
        Ok(p) => p,
        // Worktree doesn't exist on disk - cleanup would be a no-op anyway.
        Err(_) => return false,
    };
    match cwd.canonicalize() {
        Ok(cwd) => cwd.starts_with(&expected),
        // Can't verify cwd is safe - assume the worst and defer.
        Err(_) => true,
    }
}

/// Complete a stage with merge, triggering dependents on success.
///
/// This is the standard completion path for stages after acceptance criteria pass.
/// It attempts progressive merge and marks the stage as completed.
pub fn complete_with_merge(stage: &mut Stage, repo_root: &Path, work_dir: &Path) -> Result<bool> {
    match attempt_progressive_merge(stage, repo_root, work_dir)? {
        MergeOutcome::Success => {
            // Mark stage as completed - only after merge succeeds.
            stage.try_complete(None)?;
            // Re-apply only the completion-owned fields (merged, completed_commit,
            // and the Completed transition) onto the FRESH on-disk stage, so a
            // concurrent daemon/dispute write during the multi-minute acceptance
            // run is not reverted (A-5). `try_complete` recomputes duration from
            // the on-disk `started_at` (owned by the executor) and validates the
            // transition against the current on-disk status — if a dispute moved
            // the stage to NeedsAdjudication meanwhile, it correctly refuses.
            let completed_commit = stage.completed_commit.clone();
            update_stage(&stage.id, work_dir, |s| {
                s.completed_commit = completed_commit.clone();
                s.merged = true;
                s.try_complete(None)
            })?;

            println!("Stage '{}' completed!", stage.id);

            // Trigger dependent stages
            let target_branch = crate::fs::work_dir::load_config(work_dir)
                .ok()
                .flatten()
                .and_then(|c| c.base_branch());
            let target_branch =
                crate::git::branch::resolve_target_branch(&target_branch, repo_root);
            let triggered = crate::verify::transitions::trigger_dependents(
                &stage.id,
                work_dir,
                repo_root,
                &target_branch,
            )
            .context("Failed to trigger dependent stages")?;

            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  → {dep_id}");
                }
            }

            // Clean up worktree and branch after successful merge - unless
            // this session is running from inside the worktree being removed
            // (see should_defer_cleanup).
            let defer_cleanup = match std::env::current_dir() {
                Ok(cwd) => should_defer_cleanup(&cwd, repo_root, &stage.id),
                Err(_) => true,
            };

            if defer_cleanup {
                println!(
                    "  Worktree cleanup deferred to the orchestrator (session is running inside the worktree)"
                );
                println!(
                    "  If no daemon is running, clean up manually with: loom worktree remove {}",
                    stage.id
                );
            } else {
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
            }

            Ok(true)
        }
        MergeOutcome::Conflict => {
            bail!(
                "Merge conflict detected for stage '{}'.\n\
                 A resolution session has been spawned to handle the merge.\n\
                 Your work is committed on the stage branch -- this session should exit now.\n\
                 Do NOT attempt to resolve the merge conflict yourself.",
                stage.id
            );
        }
        MergeOutcome::Blocked => {
            bail!(
                "Merge blocked for stage '{}'.\n\
                 The stage has been marked MergeBlocked.\n\
                 This session should exit now. Fix the issue and run: loom stage retry {}",
                stage.id,
                stage.id
            );
        }
    }
}
