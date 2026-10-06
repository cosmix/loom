//! Human review response for a stage
//!
//! Allows a human to respond to a stage flagged for review via dispute-criteria.
//! Actions: approve (queue a fresh session), force-complete (skip acceptance), reject (block).

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::progressive_complete::complete_with_merge;
use crate::git::{worktree::find_repo_root_from_cwd, MergeGate};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::adjudication::close_open_disputes;
use crate::orchestrator::coherence::live_worker_sessions;
use crate::verify::transitions::{load_stage, update_stage};

/// Handle human review response for a stage.
///
/// One of `approve`, `force_complete`, or `reject_reason` must be provided.
/// If none are provided, shows the current review status and available actions.
pub fn human_review(
    stage_id: String,
    approve: bool,
    force_complete: bool,
    reject_reason: Option<String>,
) -> Result<()> {
    let work_dir_buf = crate::commands::common::work_dir_path()?;
    let work_dir: &Path = &work_dir_buf;

    let stage = load_stage(&stage_id, work_dir)?;

    // If no action flag is provided, show current status
    if !approve && !force_complete && reject_reason.is_none() {
        return show_review_status(&stage_id, &stage);
    }

    // Verify the stage is in NeedsHumanReview
    if stage.status != StageStatus::NeedsHumanReview {
        bail!(
            "Stage '{}' is in '{}' state. human-review requires NeedsHumanReview.",
            stage_id,
            stage.status
        );
    }

    if approve {
        handle_approve(&stage_id, work_dir)
    } else if force_complete {
        handle_force_complete(&stage_id, work_dir)
    } else if let Some(reason) = reject_reason {
        handle_reject(&stage_id, &reason, work_dir)
    } else {
        unreachable!()
    }
}

/// Show current review status and available actions.
fn show_review_status(stage_id: &str, stage: &Stage) -> Result<()> {
    if stage.status != StageStatus::NeedsHumanReview {
        bail!(
            "Stage '{}' is in '{}' state, not awaiting human review.",
            stage_id,
            stage.status
        );
    }

    println!("Stage '{stage_id}' is awaiting human review.");
    println!();
    if let Some(ref reason) = stage.review_reason {
        println!("Review reason: {reason}");
    } else {
        println!("Review reason: (none recorded)");
    }
    println!();
    println!("Available actions:");
    println!("  loom stage human-review {stage_id} --approve         Queue a fresh session with fresh fix attempts");
    println!("  loom stage human-review {stage_id} --force-complete  Skip acceptance and mark as completed");
    println!(
        "  loom stage human-review {stage_id} --reject <reason> Block the stage with a reason"
    );

    Ok(())
}

/// Approve the review: queue a fresh session with fresh fix attempts.
///
/// A contract stage reaches NeedsHumanReview by exhausting its contract
/// respawn budget; left spent, the first contract writer the requeued
/// session spawns would end without freezing and re-escalate immediately.
/// Reset inside the same locked `update_stage` closure that performs the
/// requeue, after `try_approve_review` re-validates the on-disk status and
/// before the closure returns `Ok`, matching `skip_retry::persist_retry_delta`'s
/// ordering — so a refused transition (the on-disk stage no longer in
/// NeedsHumanReview) never resets the budget, and a failed reset aborts the
/// transition instead of leaving it approved with a still-spent budget.
fn handle_approve(stage_id: &str, work_dir: &Path) -> Result<()> {
    refuse_live_worker(stage_id, work_dir)?;
    // Disputes left open when the stage escalated would shadow the one the
    // fresh session files. Closing here also repairs a crash between an
    // escalation's status write and its own close. Before the transition, so
    // a crash after it cannot leave a queued stage with stale disputes.
    if load_stage(stage_id, work_dir)?.status == StageStatus::NeedsHumanReview {
        close_open_disputes(work_dir, stage_id);
    }
    update_stage(stage_id, work_dir, |stage| {
        stage.try_approve_review()?;
        stage.fix_attempts = 0;
        stage.stall_recoveries = 0;
        super::skip_retry::reset_contract_budget(stage, work_dir)?;
        Ok(())
    })?;

    println!("Stage '{stage_id}' approved: queued for a fresh session with fresh fix attempts.");

    Ok(())
}

/// Refuse to approve while the stage has a live worker session: the executor
/// would adopt it, by the same predicate, instead of spawning a fresh session.
fn refuse_live_worker(stage_id: &str, work_dir: &Path) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)?;
    let live = live_worker_sessions(work_dir, &stage)?;
    if live.is_empty() {
        return Ok(());
    }
    let ids: Vec<&str> = live.iter().map(|session| session.id.as_str()).collect();
    bail!(
        "Stage '{stage_id}' still has live worker session(s) {}; approving would queue the \
         stage onto that agent instead of a fresh session. Run 'loom stage reset {stage_id} \
         --kill-session' to take it down (this also resets the stage), or approve once it \
         has exited.",
        ids.join(", ")
    )
}

/// Force-complete the review: skip acceptance criteria and merge, then mark as completed.
///
/// Merge runs BEFORE the Completed transition, so a conflict takes the legal
/// Executing→MergeConflict/MergeBlocked edges. No operator proof guards this command: `human-review` is not relayed and
/// writes the stage file itself. Its authority rests on every session capsule
/// denying writes to the state directory, so a sandboxed agent cannot change
/// the stage file first (`every_session_kind_denies_writes_to_the_state_directory`).
fn handle_force_complete(stage_id: &str, work_dir: &Path) -> Result<()> {
    eprintln!(
        "WARNING: Force-completing stage '{stage_id}' without acceptance criteria verification."
    );

    // Go to Executing directly (try_approve_review targets Queued) so every
    // merge outcome is a legal transition: Executing → MergeConflict |
    // MergeBlocked | Completed (complete_with_merge runs try_complete(None)).
    let mut stage = update_stage(stage_id, work_dir, |stage| {
        stage.try_transition(StageStatus::Executing)?;
        stage.review_reason = None;
        Ok(())
    })?;

    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());

    // Progressive merge + completion: Success moves Executing → Completed and
    // triggers dependents; Conflict/Blocked move to the merge state and save,
    // and stay command failures so automation cannot mistake them for a
    // force-completion. The reviewed branch skips the control-path gate.
    complete_with_merge(&mut stage, &repo_root, work_dir, MergeGate::Bypass)?;
    println!("Stage '{stage_id}' force-completed and merged successfully.");

    Ok(())
}

/// Reject the review: block the stage with a reason.
fn handle_reject(stage_id: &str, reason: &str, work_dir: &Path) -> Result<()> {
    update_stage(stage_id, work_dir, |stage| {
        stage.try_reject_review(reason.to_string())?;
        stage.close_reason = Some(reason.to_string());
        stage.failure_info = None;
        Ok(())
    })?;

    println!("Stage '{stage_id}' rejected and blocked.");
    println!("Reason: {reason}");

    Ok(())
}

#[cfg(test)]
#[path = "human_review_tests.rs"]
mod tests;
