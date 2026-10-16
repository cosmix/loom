//! The tail of a progressive merge that landed: mark the stage completed,
//! trigger dependents and clean up.

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::CleanupConfig;
use crate::models::stage::Stage;
use crate::orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle};
use crate::verify::transitions::update_stage;

/// Mark a merged stage completed, trigger its dependents and clean up.
pub(super) fn finish_completion(
    stage: &mut Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<bool> {
    // Mark stage as completed - only after merge succeeds.
    stage.try_complete(None)?;
    // Re-apply only the merged flag and Completed transition after checking
    // the recorded completion proof on the FRESH on-disk stage, so a
    // concurrent daemon/dispute write during the multi-minute acceptance
    // run is not reverted (A-5). `try_complete` recomputes duration from
    // the on-disk `started_at` (owned by the executor) and validates the
    // transition against the current on-disk status — if a dispute moved
    // the stage to NeedsAdjudication meanwhile, it correctly refuses.
    let completed_commit = stage
        .completed_commit
        .as_deref()
        .context("completion proof is missing during merge finalization")?;
    update_stage(&stage.id, work_dir, |s| {
        anyhow::ensure!(
            s.completed_commit.as_deref() == Some(completed_commit),
            "completion proof changed during merge finalization"
        );
        s.merged = true;
        s.try_complete(None)
    })?;

    println!("Stage '{}' completed!", stage.id);
    trigger_and_clean_up(&stage.id, repo_root, work_dir)?;
    Ok(true)
}

/// Reconcile the base, trigger dependent stages, then clean up the worktree,
/// branch and source-graph overlay of the merged stage.
fn trigger_and_clean_up(stage_id: &str, repo_root: &Path, work_dir: &Path) -> Result<()> {
    let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
    MergeLifecycle::new(stage_id, repo_root, work_dir).reconcile_base(&target_branch);

    let triggered = crate::verify::transitions::trigger_dependents(
        stage_id,
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

    // Cleanup after a verified merge (see `MergeLifecycle::cleanup`); it
    // honours the in-worktree deferral itself. `verbose: false` because
    // `print_cleanup_outcome` below is the single place that reports what
    // cleanup did — `cleanup_after_merge` would otherwise print the same
    // "Removed worktree"/"Deleted branch" lines itself.
    let cleanup_config = CleanupConfig {
        verbose: false,
        force_worktree_removal: false,
        force_branch_deletion: false,
        prune_worktrees: true,
    };
    let outcome =
        MergeLifecycle::new(stage_id, repo_root, work_dir).cleanup(&target_branch, &cleanup_config);
    print_cleanup_outcome(stage_id, outcome);
    Ok(())
}

/// Print `complete_with_merge`'s post-merge cleanup summary for `outcome`,
/// matching the pre-refactor `cleanup_after_merge` output.
fn print_cleanup_outcome(stage_id: &str, outcome: CleanupOutcome) {
    match outcome {
        CleanupOutcome::NothingToDo => {}
        CleanupOutcome::Deferred { reason } => {
            println!("  Worktree cleanup deferred to the orchestrator: {reason}");
            println!(
                "  If no daemon is running, clean up manually with: loom worktree remove {stage_id}"
            );
        }
        CleanupOutcome::Refused { reason } => {
            eprintln!("  Warning: Cleanup refused: {reason}");
            eprintln!("  You can manually clean up with: loom worktree remove {stage_id}");
        }
        // `result.warnings` is not surfaced: `MergeLifecycle::cleanup` reaches
        // `Done` only via `cleanup_after_merge`, which always builds an empty
        // `warnings` vec on its `Ok` path (only `cleanup_multiple_stages`
        // ever populates it) — dead on this path.
        CleanupOutcome::Done(result) => {
            if result.worktree_removed {
                println!("  Removed worktree: .worktrees/{stage_id}");
            }
            if result.branch_deleted {
                println!("  Deleted branch: {}", branch_name_for_stage(stage_id));
            }
        }
        CleanupOutcome::Failed(e) => {
            eprintln!("  Warning: Failed to clean up stage resources: {e}");
            eprintln!("  You can manually clean up with: loom worktree remove {stage_id}");
        }
    }
}
