//! The tails of `loom stage merge`: record a block, or mark a landed merge
//! complete, trigger dependents and clean up.

use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::git::{merge_stage, MergeBlock, MergeGate, MergeResult};
use crate::models::stage::Stage;
use crate::verify::transitions::{trigger_dependents, update_stage};

use super::conflict::record_conflict_and_report;
use super::finish::finish_merge_and_report;
use super::next_step::report_merge_error;
use crate::commands::stage::progressive_complete::{
    note_merge_stash, report_merge_block, route_held_to_review, route_zero_commit_to_review,
    zero_commit_review_reason,
};

/// The main repository root, followed out of a worktree through the state
/// directory's symlink.
pub(super) fn main_repo_root(work_dir: &Path) -> Result<PathBuf> {
    crate::fs::work_dir::WorkDir::new(work_dir)?
        .main_project_root()
        .ok_or_else(|| {
            anyhow!("Could not resolve the main repository root from the state directory")
        })
}

/// Persist a typed merge block on the fresh on-disk stage and say what retries
/// it. The stage stays unmerged.
/// A stage already merged is refused: a late block must not move it backward.
pub(super) fn record_block(stage_id: &str, work_dir: &Path, block: MergeBlock) -> Result<()> {
    let persisted = block.clone();
    update_stage(stage_id, work_dir, |s| {
        if s.merged {
            bail!("stage '{stage_id}' is already merged; the merge block is not recorded");
        }
        s.block_merge(persisted.clone());
        Ok(())
    })?;
    report_merge_block(stage_id, &block);
    Ok(())
}

/// Route the stage to human review because the control-path gate held its
/// branch, say how an operator releases it, and fail the command: the merge
/// did not happen.
pub(super) fn hold_for_review(stage_id: &str, work_dir: &Path, reason: &str) -> Result<()> {
    route_held_to_review(stage_id, work_dir, reason)?;
    bail!("Stage '{stage_id}' merge held for human review: the branch touches a control path")
}

/// Merge `stage` into `target_branch` for `loom stage merge` and act on the
/// outcome: complete it, record a block or conflict, hold it for review, or
/// report the error (`fix_attempts` was already persisted by the caller).
pub(super) fn attempt_retried_merge(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<()> {
    let stage_id = stage.id.as_str();
    if let Some(reason) = zero_commit_review_reason(stage, target_branch, repo_root) {
        route_zero_commit_to_review(stage_id, work_dir, &reason)?;
        bail!(
            "Stage '{stage_id}' has no commits to merge and was routed to human review: {reason}"
        );
    }
    match merge_stage(
        stage_id,
        target_branch,
        repo_root,
        work_dir,
        MergeGate::Enforce,
    ) {
        Ok(MergeResult::Success {
            files_changed,
            insertions,
            deletions,
            stash,
        }) => {
            println!("Merge successful!");
            println!("  {files_changed} files changed, +{insertions} -{deletions}");
            note_merge_stash(stage_id, work_dir, stash);
            let done = format!("Stage '{stage_id}' merge complete! (Completed, merged: true)");
            complete_retried_merge(stage, work_dir, repo_root, target_branch, &done)
        }
        Ok(MergeResult::Blocked(block)) => record_block(stage_id, work_dir, block),
        Ok(MergeResult::AlreadyUpToDate) => {
            println!("Branch is already up to date with {target_branch}.");
            let done = format!("Stage '{stage_id}' marked as merged.");
            complete_retried_merge(stage, work_dir, repo_root, target_branch, &done)
        }
        Ok(MergeResult::Conflict { conflicting_files }) => {
            record_conflict_and_report(stage, work_dir, repo_root, &conflicting_files)
        }
        Ok(MergeResult::Held { reason }) => hold_for_review(stage_id, work_dir, &reason),
        Err(e) => {
            report_merge_error(stage, work_dir, repo_root, &e);
            Ok(())
        }
    }
}

/// Mark a resolved merge complete after ancestry proves it landed, then
/// trigger dependents and clean up. Cleanup defers when this runs inside the
/// stage worktree, which the resolver session still occupies.
pub(super) fn complete_resolved_merge(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<()> {
    let stage_id = stage.id.as_str();
    prove_and_mark_merged(stage, work_dir, repo_root, target_branch)?;
    println!("Stage '{stage_id}' merge conflict resolution complete!");
    println!("  Status: Completed (merged: true)");
    trigger_and_report(stage_id, work_dir, repo_root, target_branch)?;
    finish_merge_and_report(stage_id, repo_root, work_dir, target_branch);
    Ok(())
}

/// Clear the conflict flag and merge block and mark the stage completed+merged
/// once ancestry proves its commit is in the target branch, then trigger
/// dependents and clean up. A branch with no commits beyond the target merges
/// as a no-op, so success alone proves nothing.
pub(super) fn complete_retried_merge(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
    done_message: &str,
) -> Result<()> {
    let stage_id = stage.id.as_str();
    prove_and_mark_merged(stage, work_dir, repo_root, target_branch)?;
    println!();
    println!("{done_message}");
    trigger_and_report(stage_id, work_dir, repo_root, target_branch)?;
    finish_merge_and_report(stage_id, repo_root, work_dir, target_branch);
    Ok(())
}

/// Write `merged = true` and `Completed` on the fresh on-disk stage (A-5), but
/// only after ancestry proves its recorded completion commit is in the target.
/// A refusal writes nothing and propagates.
///
/// PHANTOM-MERGE INVARIANT: no `merged = true` without that proof.
fn prove_and_mark_merged(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<()> {
    anyhow::ensure!(
        stage.completed_commit.is_some(),
        "completion proof is missing; rerun completion before landing"
    );
    let verified = crate::commands::stage::merge_verify::verify_or_derive_completed_commit(
        stage,
        target_branch,
        repo_root,
    )?;
    update_stage(&stage.id, work_dir, |s| {
        anyhow::ensure!(
            s.completed_commit.as_deref() == Some(verified.commit.as_str()),
            "completion proof changed during merge finalization"
        );
        s.merge_conflict = false;
        s.clear_merge_block();
        s.try_complete_merge()
    })?;
    Ok(())
}

/// Trigger dependent stages and print which ones started, if any. Shared by
/// every merge-completion path so the reporting stays identical across them.
pub(super) fn trigger_and_report(
    stage_id: &str,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<()> {
    let triggered = trigger_dependents(stage_id, work_dir, repo_root, target_branch)
        .context("Failed to trigger dependent stages")?;
    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  -> {dep_id}");
        }
    }
    Ok(())
}
