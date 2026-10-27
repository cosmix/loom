//! Where a stage goes once a verdict on one of its disputes is applied: back
//! to `Queued`, or held in `NeedsAdjudication` while a sibling is unanswered.

use anyhow::Result;
use std::path::Path;

use crate::models::stage::{Stage, StageStatus};

use super::AdjudicatorRegistry;

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

/// Re-queue the stage, unless another dispute on it still has no verdict.
///
/// `job_for_dispute` only schedules a dispute whose stage is
/// `NeedsAdjudication`, so the stage must stay there until the LAST
/// unanswered dispute is judged; only that verdict re-queues it. The
/// dispute currently being applied already has its `verdict.md` on disk, so
/// `scan_pending_requests` does not count it among the remainder.
pub(super) fn requeue_or_hold_for_remaining_disputes(
    work_dir: &Path,
    stage: &mut Stage,
) -> Result<()> {
    if stage.status == StageStatus::NeedsHumanReview {
        // A Reject verdict on a sibling dispute already escalated this stage;
        // a later Accept/NeedsMoreEvidence verdict must not re-queue over it.
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage.id,
            "stage already NeedsHumanReview; not re-queueing or holding for remaining disputes",
        );
        return Ok(());
    }
    let remaining = AdjudicatorRegistry::new().unanswered_disputes(work_dir, &stage.id)?;
    if remaining == 0 {
        return transition_to_queued(stage);
    }
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
    Ok(())
}
