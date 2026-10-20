//! Persisting a progressive merge outcome that did not land: blocked, held
//! for review, or conflicting. Each writer re-applies only the fields the
//! completion owns onto the fresh on-disk stage (A-5), after its `Claim`
//! re-checks the completion commit and steps the stage out of review when an
//! operator lands it.

use anyhow::Result;
use chrono::Utc;
use std::path::Path;

use super::landing::Claim;
use super::report::{report_merge_block, report_merge_hold};
use super::MergeOutcome;
use crate::git::branch::branch_name_for_stage;
use crate::git::merge::MergeBlock;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::Stage;
use crate::verify::transitions::update_stage;

/// The merge could not be attempted: block the stage and say how to retry.
pub(super) fn block_failed_merge(
    stage: &mut Stage,
    work_dir: &Path,
    claim: &Claim,
    error: &anyhow::Error,
) -> Result<MergeOutcome> {
    eprintln!("Progressive merge failed: {error}");
    let outcome = mark_blocked(stage, work_dir, claim, &format!("{error:#}"))?;
    eprintln!("Stage '{}' marked as MergeBlocked", stage.id);
    eprintln!("  Fix the issue and run: loom stage retry {}", stage.id);
    Ok(outcome)
}

/// A missing stage branch cannot prove anything landed: block the merge
/// instead of recording it as merged.
pub(super) fn block_missing_branch(
    stage: &mut Stage,
    work_dir: &Path,
    claim: &Claim,
) -> Result<MergeOutcome> {
    tracing::error!(
        stage_id = %stage.id,
        "Progressive merge: branch missing — cannot verify merge succeeded"
    );
    let reason = format!(
        "branch {} is missing, so the merge cannot be verified",
        branch_name_for_stage(&stage.id)
    );
    mark_blocked(stage, work_dir, claim, &reason)
}

/// Enter `MergeBlocked` without a typed block: an untyped failure replaces any
/// typed block an earlier attempt left, and `reason` goes into `failure_info`
/// so `loom status` shows it, as the daemon's merge failures do.
fn block_untyped(stage: &mut Stage, reason: &str) -> Result<()> {
    stage.clear_merge_block();
    stage.try_mark_merge_blocked()?;
    stage.failure_info = Some(FailureInfo {
        failure_type: FailureType::InfrastructureError,
        detected_at: Utc::now(),
        evidence: reason.lines().map(str::to_owned).collect(),
    });
    Ok(())
}

/// Re-apply only the merge-block transition onto the fresh on-disk stage (A-5).
fn mark_blocked(
    stage: &mut Stage,
    work_dir: &Path,
    claim: &Claim,
    reason: &str,
) -> Result<MergeOutcome> {
    block_untyped(stage, reason)?;
    update_stage(&stage.id, work_dir, |s| {
        claim.apply(s)?;
        block_untyped(s, reason)
    })?;
    Ok(MergeOutcome::Blocked)
}

/// Route the stage to `NeedsHumanReview` on the fresh on-disk stage (A-5),
/// and report the hold. Nothing was merged.
pub(super) fn hold_for_review(
    stage: &mut Stage,
    work_dir: &Path,
    claim: &Claim,
    reason: &str,
) -> Result<MergeOutcome> {
    stage.route_to_review(reason);
    update_stage(&stage.id, work_dir, |s| {
        claim.apply(s)?;
        s.route_to_review(reason);
        Ok(())
    })?;
    report_merge_hold(&stage.id, reason);
    Ok(MergeOutcome::Held)
}

/// Persist a typed merge block on the fresh on-disk stage (A-5), and report
/// it. The merge was not attempted past the block, so the checkout is
/// untouched.
pub(super) fn hold_blocked_merge(
    stage: &mut Stage,
    work_dir: &Path,
    claim: &Claim,
    block: MergeBlock,
) -> Result<MergeOutcome> {
    stage.block_merge(block.clone());
    let persisted = block.clone();
    update_stage(&stage.id, work_dir, |s| {
        claim.apply(s)?;
        s.block_merge(persisted.clone());
        Ok(())
    })?;
    report_merge_block(&stage.id, &block);
    Ok(MergeOutcome::Blocked)
}

/// Where a conflicting merge was attempted.
pub(super) struct ConflictTarget<'a> {
    pub(super) merge_point: &'a str,
    pub(super) repo_root: &'a Path,
    pub(super) work_dir: &'a Path,
}

/// Persist `MergeConflict` (A-5), report the conflicting files, and start a
/// merge resolver for the stage.
pub(super) fn record_conflict(
    stage: &mut Stage,
    conflicting_files: &[String],
    claim: &Claim,
    target: &ConflictTarget<'_>,
) -> Result<MergeOutcome> {
    println!("  ✗ Merge conflict detected!");
    println!("    Conflicting files:");
    for file in conflicting_files {
        println!("      - {file}");
    }
    println!();
    println!("    Stage transitioning to MergeConflict status.");
    stage.try_mark_merge_conflict()?;
    update_stage(&stage.id, target.work_dir, |s| {
        claim.apply(s)?;
        s.clear_merge_block();
        s.try_mark_merge_conflict()
    })?;
    spawn_resolver_and_report(stage, conflicting_files, target);
    Ok(MergeOutcome::Conflict)
}

/// Try to auto-spawn a merge resolver session. `merge_stage` computes the
/// merge without touching the checkout, so the resolver starts from a clean
/// stage worktree.
fn spawn_resolver_and_report(
    stage: &Stage,
    conflicting_files: &[String],
    target: &ConflictTarget<'_>,
) {
    use crate::commands::stage::merge_resolver::{spawn_merge_resolver, MergeResolverResult};
    match spawn_merge_resolver(
        stage,
        conflicting_files,
        target.merge_point,
        target.repo_root,
        target.work_dir,
    ) {
        Ok(MergeResolverResult::DaemonManaged) => {
            println!("    Daemon is running - merge resolution will be handled automatically.");
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
}

#[cfg(test)]
#[path = "mark_blocked_tests.rs"]
mod mark_blocked_tests;
