//! Progressive merge integration for stage completion
//!
//! This module handles the git merge operations that occur when a stage
//! completes successfully with passing acceptance criteria.

use anyhow::{bail, Result};
use std::path::Path;

use crate::git::merge::MergeGate;
use crate::models::stage::Stage;
use crate::orchestrator::merge_lifecycle::MergeLifecycle;
use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};

mod finish;
mod guard;
pub(super) mod landing;
mod operator_commit;
mod outcome;
mod report;
pub(super) use guard::{route_zero_commit_to_review, zero_commit_review_reason};
use landing::Claim;
pub use landing::Landing;
pub(super) use operator_commit::{
    branch_work, report_recorded_commit, warn_unconfirmed, BranchWork,
};
use outcome::{
    block_failed_merge, block_missing_branch, hold_blocked_merge, hold_for_review, record_conflict,
    ConflictTarget,
};
pub(super) use report::{note_merge_stash, report_merge_block, route_held_to_review};

/// Result of attempting to merge a completed stage
pub enum MergeOutcome {
    /// Merge succeeded - stage can be marked completed
    Success,
    /// Merge conflict - stage should be marked MergeConflict
    Conflict,
    /// Merge blocked - stage should be marked MergeBlocked
    Blocked,
    /// The control-path gate held the branch - stage already routed to
    /// NeedsHumanReview
    Held,
    /// The branch has no commit beyond the merge point - stage already routed
    /// to NeedsHumanReview, nothing merged
    NoCommits,
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
/// - `MergeOutcome::Held` if the landing gates the merge and the branch touches
///   a control path (stage already routed to NeedsHumanReview)
/// - `MergeOutcome::NoCommits` if the landing gates the merge and the branch
///   has no commit beyond the merge point (stage already routed to
///   NeedsHumanReview)
///
/// An operator's `Landing::Reviewed` keeps the on-disk stage in
/// NeedsHumanReview until the outcome is written; only the in-memory copy
/// enters Executing before the merge.
///
/// `merged` is set only after `verify_or_derive_completed_commit` proves the
/// stage's commit is in the merge point; a refusal returns `Err` and leaves the
/// stage unmerged.
pub fn attempt_progressive_merge(
    stage: &mut Stage,
    repo_root: &Path,
    work_dir: &Path,
    landing: Landing,
) -> Result<MergeOutcome> {
    let merge_point = get_merge_point(work_dir)?;
    let gate = landing.gate();

    if route_if_no_commits(stage, &merge_point, repo_root, work_dir, gate)? {
        return Ok(MergeOutcome::NoCommits);
    }
    let claim = Claim::new(stage, landing);
    landing.enter(stage)?;

    println!("Attempting progressive merge into '{merge_point}'...");
    #[cfg(test)]
    landing::run_window_probe();
    let merged = merge_completed_stage(stage, repo_root, &merge_point, gate);
    let at = ConflictTarget {
        merge_point: &merge_point,
        repo_root,
        work_dir,
    };
    record_merge_result(stage, merged, &claim, &at)
}

/// Record what the merge did: mark a landed branch merged in memory (the
/// caller writes the completion), or persist the outcome that stopped it.
fn record_merge_result(
    stage: &mut Stage,
    merged: Result<ProgressiveMergeResult>,
    claim: &Claim,
    at: &ConflictTarget<'_>,
) -> Result<MergeOutcome> {
    let (merge_point, repo_root, work_dir) = (at.merge_point, at.repo_root, at.work_dir);
    match merged {
        Ok(ProgressiveMergeResult::Success {
            files_changed,
            stash,
        }) => {
            println!("  ✓ Merged {files_changed} file(s) into '{merge_point}'");
            note_merge_stash(&stage.id, work_dir, stash);
            mark_landed(stage, merge_point, repo_root)
        }
        Ok(ProgressiveMergeResult::AlreadyMerged) => {
            println!("  ✓ Already up to date with '{merge_point}'");
            mark_landed(stage, merge_point, repo_root)
        }
        Ok(ProgressiveMergeResult::NoBranch) => block_missing_branch(stage, work_dir, claim),
        Ok(ProgressiveMergeResult::Blocked(block)) => {
            hold_blocked_merge(stage, work_dir, claim, block)
        }
        Ok(ProgressiveMergeResult::Held { reason }) => {
            hold_for_review(stage, work_dir, claim, &reason)
        }
        Ok(ProgressiveMergeResult::Conflict { conflicting_files }) => {
            record_conflict(stage, &conflicting_files, claim, at)
        }
        Err(e) => block_failed_merge(stage, work_dir, claim, &e),
    }
}

/// Under `Enforce`, route a stage whose branch has no commit beyond
/// `merge_point` to human review. Returns whether it did.
fn route_if_no_commits(
    stage: &Stage,
    merge_point: &str,
    repo_root: &Path,
    work_dir: &Path,
    gate: MergeGate,
) -> Result<bool> {
    if gate != MergeGate::Enforce {
        return Ok(false);
    }
    let Some(reason) = zero_commit_review_reason(stage, merge_point, repo_root) else {
        return Ok(false);
    };
    route_zero_commit_to_review(&stage.id, work_dir, &reason)?;
    Ok(true)
}

/// Mark `stage` merged, unless git ancestry does not show its commit in
/// `merge_point`: then the refusal propagates and the stage stays unmerged.
fn mark_landed(stage: &mut Stage, merge_point: &str, repo_root: &Path) -> Result<MergeOutcome> {
    super::merge_verify::verify_or_derive_completed_commit(stage, merge_point, repo_root)?;
    stage.merged = true;
    Ok(MergeOutcome::Success)
}

/// Whether worktree cleanup for `stage_id` must be deferred rather than run
/// now. See `merge_lifecycle::should_defer_cleanup` for the real
/// implementation; production code now reaches it only via
/// `MergeLifecycle::cleanup`. This delegation stays test-only (`cfg(test)`)
/// so `tests/progressive_complete.rs`'s regression coverage for the
/// deferral gate keeps compiling without leaving a dead production symbol.
#[cfg(test)]
pub(super) fn should_defer_cleanup(cwd: &Path, repo_root: &Path, stage_id: &str) -> bool {
    crate::orchestrator::merge_lifecycle::should_defer_cleanup(cwd, repo_root, stage_id)
}

/// Complete a stage with merge, triggering dependents on success.
///
/// This is the standard completion path for stages after acceptance criteria pass.
/// It attempts progressive merge and marks the stage as completed. `landing`
/// is `Reviewed` only for an operator's forced completion of a reviewed branch.
pub fn complete_with_merge(
    stage: &mut Stage,
    repo_root: &Path,
    work_dir: &Path,
    landing: Landing,
) -> Result<bool> {
    MergeLifecycle::new(&stage.id, repo_root, work_dir).reconcile_overlay();

    match attempt_progressive_merge(stage, repo_root, work_dir, landing)? {
        MergeOutcome::Success => finish::finish_completion(stage, repo_root, work_dir, landing),
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
        MergeOutcome::NoCommits => {
            bail!(
                "Stage '{}' has no commits beyond the merge point: the agent never committed \
                 work.\nThe stage has been routed to NeedsHumanReview.\n\
                 This session should exit now; a person decides.",
                stage.id
            );
        }
        MergeOutcome::Held => {
            bail!(
                "Merge held for stage '{}': the branch touches a control path.\n\
                 The stage has been routed to NeedsHumanReview.\n\
                 This session should exit now; a person releases it.",
                stage.id
            );
        }
    }
}
