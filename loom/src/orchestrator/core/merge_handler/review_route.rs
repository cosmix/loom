//! Routing a stage to `NeedsHumanReview`. The spawn loop's routings are
//! guarded: the stage copy the loop loaded can go stale while it runs git, and
//! a stage that left `MergeConflict`/`MergeBlocked` meanwhile (a concurrent
//! `loom stage merge` merged it, say) must never be overwritten.

use chrono::Utc;

use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};

/// What a routing to human review did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReviewRoute {
    /// `NeedsHumanReview` is on disk and in the graph.
    Routed,
    /// The fresh on-disk stage had left its merge state; nothing was written.
    StageMovedOn,
    /// The stage file could not be updated; it keeps its status, so the next
    /// spawn-loop pass meets the stage again.
    NotSaved,
}

/// Whether `status` is one the spawn loop gives merge resolvers.
pub(super) fn awaits_merge(status: &StageStatus) -> bool {
    matches!(
        status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    )
}

/// Force `stage` into `NeedsHumanReview` with `reason`, recording
/// `failure_type` as its `failure_info` when given; the failure it replaces
/// goes into `issue_history`.
fn record_review(stage: &mut Stage, reason: &str, failure_type: Option<FailureType>) {
    stage.force_status_with_reason(StageStatus::NeedsHumanReview, reason);
    stage.replace_review_reason(reason.to_string());
    if let Some(failure_type) = failure_type {
        stage.replace_failure(FailureInfo {
            failure_type,
            detected_at: Utc::now(),
            evidence: vec![reason.to_string()],
        });
    }
}

impl Orchestrator {
    /// Persist `NeedsHumanReview` with `reason` whatever the stage's status,
    /// mirrored into the graph; records `failure_type` as `failure_info` when
    /// given. Returns whether it was saved. `try_auto_merge` routes a
    /// `Completed` stage this way.
    pub(super) fn route_to_human_review(
        &mut self,
        stage_id: &str,
        reason: String,
        failure_type: Option<FailureType>,
    ) -> bool {
        let route = self.route_to_review_if(stage_id, reason, failure_type, |_| true);
        route == ReviewRoute::Routed
    }

    /// [`Self::route_to_human_review`] for a stage the spawn loop loaded in
    /// `MergeConflict`/`MergeBlocked`: written only while the fresh on-disk
    /// stage, read under the stage-file lock, is still in one of them.
    pub(super) fn route_merge_stage_to_review(
        &mut self,
        stage_id: &str,
        reason: String,
        failure_type: Option<FailureType>,
    ) -> ReviewRoute {
        self.route_to_review_if(stage_id, reason, failure_type, awaits_merge)
    }

    fn route_to_review_if(
        &mut self,
        stage_id: &str,
        reason: String,
        failure_type: Option<FailureType>,
        may_route: fn(&StageStatus) -> bool,
    ) -> ReviewRoute {
        let mut moved_on = None;
        let updated = self.update_stage(stage_id, |stage| {
            if stage.is_final_merged() || !may_route(&stage.status) {
                moved_on = Some(stage.status.clone());
                anyhow::bail!("stage '{stage_id}' is {} on disk", stage.status);
            }
            record_review(stage, &reason, failure_type);
            Ok(())
        });
        if let Some(status) = moved_on {
            tracing::info!(
                stage_id = %stage_id,
                %status,
                "Stage left its merge state meanwhile; not routing it to human review"
            );
            return ReviewRoute::StageMovedOn;
        }
        if let Err(error) = updated {
            tracing::warn!(
                stage_id = %stage_id,
                %error,
                "Failed to save stage after routing to human review"
            );
            return ReviewRoute::NotSaved;
        }
        if let Err(e) = self
            .graph
            .mark_status(stage_id, StageStatus::NeedsHumanReview)
        {
            tracing::warn!(
                stage_id = %stage_id,
                error = %e,
                "Failed to mark stage NeedsHumanReview in graph after routing to human review"
            );
        }
        ReviewRoute::Routed
    }

    /// Route a stage whose merge-resolver budget is exhausted to
    /// `NeedsHumanReview` and persist it.
    ///
    /// `MergeConflict`/`MergeBlocked -> NeedsHumanReview` is not a legal edge, so
    /// this uses the sanctioned forced-assignment path. After escalation the
    /// stage is no longer in MergeConflict/MergeBlocked, so the spawn loop stops
    /// considering it and respawning ceases. A stage that left its merge state
    /// meanwhile is left alone, counter included.
    pub(super) fn escalate_merge_resolver_exhausted(
        &mut self,
        stage_id: &str,
        failed_attempts: u32,
    ) {
        let steps = self.manual_merge_steps(stage_id);
        let reason = format!("merge resolver gave up after {failed_attempts} attempt(s): {steps}");
        tracing::error!(
            stage_id = %stage_id,
            failed_attempts = %failed_attempts,
            "Merge-resolver attempt cap reached; routing stage to NeedsHumanReview"
        );
        if self.route_merge_stage_to_review(stage_id, reason, None) != ReviewRoute::Routed {
            return;
        }

        // Remove any lingering active session and clear the counter so a future
        // manual re-merge starts fresh.
        self.active_sessions.remove(stage_id);
        self.clear_merge_resolver_attempts(stage_id);

        clear_status_line();
        eprintln!(
            "Stage '{stage_id}' needs human review: merge resolution failed after \
             {failed_attempts} attempt(s). To finish it, {steps}."
        );
    }
}
