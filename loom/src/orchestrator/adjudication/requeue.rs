//! Where a stage goes once a verdict on one of its disputes is applied: back
//! to `Queued`, or held in `NeedsAdjudication` while a sibling is unanswered.

use anyhow::Result;
use std::path::Path;

use crate::models::dispute::{applied_marker, dispute_dir, request_file};
use crate::models::stage::{Stage, StageStatus};

use super::{apply_receipt::verdict_receipt, dispute_ids, is_closed};

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

/// Re-queue the stage, unless another open dispute has no applied outcome.
///
/// `job_for_dispute` only schedules a dispute whose stage is
/// `NeedsAdjudication`, so the stage must stay there until the LAST
/// unapplied dispute is judged; only that verdict re-queues it. This includes
/// siblings whose verdicts were recorded before the current one was applied.
/// The current dispute is the one open, unapplied entry excluded from count.
/// A request no filing published is not counted: `job_for_dispute` will never
/// judge it, so holding for it would leave the stage waiting with nothing open.
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
    let root = work_dir.join("disputes");
    let all_unapplied = dispute_ids(&root.join(&stage.id))
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
        .count();
    anyhow::ensure!(
        all_unapplied > 0,
        "no open dispute owns verdict application"
    );
    let remaining = all_unapplied - 1;
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
