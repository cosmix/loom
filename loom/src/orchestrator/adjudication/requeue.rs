//! Where a stage goes once a verdict on one of its disputes is applied: back
//! to `Queued`, held in `NeedsAdjudication` while a sibling is unanswered, or
//! held there with no session for the daemon's completion run when the last
//! verdict changed nothing the agent must act on.
//!
//! The completion hold is derived, never stored: `NeedsAdjudication`, no
//! session, every dispute applied. A crash anywhere after the verdict persists
//! therefore leaves nothing stale; the daemon's next sweep finds the stage.

use anyhow::Result;
use std::path::Path;

use crate::fs::dispute_lock::lock_stage_dispute_dir_for_daemon;
use crate::models::dispute::{applied_marker, dispute_dir, request_file};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, update_stage};

use super::{apply_receipt::verdict_receipt, dispute_ids, feedback, is_closed};

fn transition_to_queued(stage: &mut Stage) -> Result<()> {
    let target = StageStatus::Queued;
    // NeedsAdjudication → Queued is a legal edge. Any OTHER status must be
    // left untouched: it means a verdict on an earlier, sibling dispute
    // already moved the stage (e.g. a Reject to NeedsHumanReview), and
    // forcing Queued here would silently erase that escalation.
    if stage.status.can_transition_to(&target) {
        stage.try_transition(target)?;
    } else {
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage.id,
            status = %stage.status,
            "refusing to force stage to Queued from a non-NeedsAdjudication status",
        );
    }
    Ok(())
}

/// Open disputes of `stage` with no applied outcome: published, not closed,
/// no `applied.marker`, no verdict receipt. A request no filing published is
/// not counted: `job_for_dispute` will never judge it, so holding for it
/// would leave the stage waiting with nothing open. A dispute directory that
/// cannot be listed counts as none, as `dispute_ids` does.
fn unapplied_open_disputes(work_dir: &Path, stage: &Stage) -> usize {
    let root = work_dir.join("disputes");
    dispute_ids(&root.join(&stage.id))
        .into_iter()
        .filter(|id| {
            request_file(&root, &stage.id, *id).exists()
                && !is_closed(&dispute_dir(&root, &stage.id, *id))
                && !applied_marker(&root, &stage.id, *id).exists()
                && !stage
                    .tally
                    .request_receipts
                    .contains_key(&verdict_receipt(*id))
                && !crate::stage_service::dispute_is_orphaned(stage, *id)
        })
        .count()
}

/// Unapplied disputes other than the one whose verdict is being applied, or
/// `None` when a sibling verdict already escalated the stage to a human.
fn siblings_awaiting_verdict(work_dir: &Path, stage: &Stage) -> Result<Option<usize>> {
    if stage.status == StageStatus::NeedsHumanReview {
        // A Reject verdict on a sibling dispute already escalated this stage;
        // a later Accept/NeedsMoreEvidence verdict must not re-queue over it.
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage.id,
            "stage already NeedsHumanReview; not re-queueing or holding for remaining disputes",
        );
        return Ok(None);
    }
    let all_unapplied = unapplied_open_disputes(work_dir, stage);
    anyhow::ensure!(
        all_unapplied > 0,
        "no open dispute owns verdict application"
    );
    Ok(Some(all_unapplied - 1))
}

fn hold_while_siblings_remain(stage: &mut Stage, remaining: usize) {
    tracing::info!(
        target: "loom::adjudication",
        stage = %stage.id,
        remaining,
        "holding stage in NeedsAdjudication: unanswered disputes remain",
    );
    if stage.status != StageStatus::NeedsAdjudication {
        stage.force_status_with_reason(
            StageStatus::NeedsAdjudication,
            "unanswered disputes remain after a verdict",
        );
    }
}

/// Re-queue the stage, unless another open dispute has no applied outcome.
///
/// `job_for_dispute` only schedules a dispute whose stage is
/// `NeedsAdjudication`, so the stage must stay there until the LAST
/// unapplied dispute is judged; only that verdict re-queues it. This includes
/// siblings whose verdicts were recorded before the current one was applied.
/// The current dispute is the one open, unapplied entry excluded from count.
pub(super) fn requeue_or_hold_for_remaining_disputes(
    work_dir: &Path,
    stage: &mut Stage,
) -> Result<()> {
    match siblings_awaiting_verdict(work_dir, stage)? {
        None => Ok(()),
        Some(0) => transition_to_queued(stage),
        Some(remaining) => {
            hold_while_siblings_remain(stage, remaining);
            Ok(())
        }
    }
}

/// As [`requeue_or_hold_for_remaining_disputes`], except that the last
/// verdict leaves the stage in `NeedsAdjudication` for the daemon's
/// completion run. It changes no field, so the saved stage differs from the
/// one on disk only by the verdict receipt.
pub(super) fn hold_for_completion_or_remaining_disputes(
    work_dir: &Path,
    stage: &mut Stage,
) -> Result<()> {
    match siblings_awaiting_verdict(work_dir, stage)? {
        None => {}
        Some(0) => tracing::info!(
            target: "loom::adjudication",
            stage = %stage.id,
            "holding stage for the daemon's completion run",
        ),
        Some(remaining) => hold_while_siblings_remain(stage, remaining),
    }
    Ok(())
}

/// `NeedsAdjudication` with no session and every dispute applied.
pub(crate) fn is_verdict_held(work_dir: &Path, stage: &Stage) -> bool {
    stage.status == StageStatus::NeedsAdjudication
        && stage.session.is_none()
        && unapplied_open_disputes(work_dir, stage) == 0
}

/// Whether the daemon should run the stage's completion: the verdict hold,
/// and not held by the operator (`loom stage hold`), whose stage the daemon
/// never completes.
pub(crate) fn is_held_for_completion(work_dir: &Path, stage: &Stage) -> bool {
    is_verdict_held(work_dir, stage) && !stage.held
}

/// Re-queue a verdict-held stage for a fresh session, appending `failures`
/// (when any) to its feedback first. Returns whether the saved stage is
/// `Queued`; `Ok(false)` changes nothing, because the stage is no longer
/// verdict-held (a new dispute, a reset, a session). The operator's hold is
/// left as it is.
///
/// Takes the stage's dispute lock, so it must never run while the caller
/// holds that lock: two `flock` descriptors in one process conflict.
pub(crate) fn hand_back_held_stage(
    work_dir: &Path,
    stage_id: &str,
    failures: &[String],
) -> Result<bool> {
    let _claim = lock_stage_dispute_dir_for_daemon(&work_dir.join("disputes").join(stage_id))?;
    let stage = load_stage(stage_id, work_dir)?;
    if !is_verdict_held(work_dir, &stage) {
        return Ok(false);
    }
    feedback::append_completion_failures(work_dir, stage_id, failures)?;
    let saved = update_stage(stage_id, work_dir, |s| {
        if s.status == StageStatus::NeedsAdjudication && s.session.is_none() {
            s.try_transition(StageStatus::Queued)?;
        }
        Ok(())
    })?;
    let moved = saved.status == StageStatus::Queued;
    if moved {
        tracing::info!(
            target: "loom::adjudication",
            stage = %stage_id,
            failures = failures.len(),
            "handed the verdict-held stage to a fresh session",
        );
    }
    Ok(moved)
}
