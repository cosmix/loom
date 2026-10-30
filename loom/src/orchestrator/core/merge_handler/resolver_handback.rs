//! A merge resolver's committed content that fails completion verification.
//! The resolver cannot repair it: a Merge-scope commit needs the merge in
//! progress, which its merge commit used up, and a Merge stage cannot dispute.
//! The daemon stops the resolver and requeues the stage, so a fresh Stage
//! session gets the failure text. A stage that cannot be requeued safely goes
//! to human review instead.

use crate::context::untrusted::inline_safe;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};

use super::resolver_stop::ResolverStop;
use super::review_route::{awaits_merge, ReviewRoute};

/// Handbacks a stage gets before the operator decides. An approval zeroes the
/// count. Revisit if fresh sessions keep fixing what verification found.
const MAX_MERGE_HANDBACKS: u32 = 2;

/// Longest refusal stored in the stage file and signal.
const MAX_REFUSAL_BYTES: usize = 8_000;

impl Orchestrator {
    /// Hand `stage_id`'s committed merge back to a stage session because its
    /// content failed completion verification with `refusal`. Every resolver
    /// of the stage is stopped first. `Ok` is the note the resolver's ledger
    /// entry or the daemon log carries; `Err` is the refusal.
    ///
    /// The stage is held for the operator instead when a resolver may still
    /// be running, a live worker session exists (the executor would adopt it
    /// instead of spawning), or the stage used its handbacks: a fresh session
    /// must never share the worktree with a live writer, and the handback
    /// must not loop.
    ///
    /// Every `Err` of `verify_resolved_completion` counts as a failure here,
    /// including a transient one such as a fingerprint or save error. That is
    /// deliberate and bounded: the fresh session's `loom stage complete`
    /// verifies from scratch and passes once the error clears, and the cap
    /// holds a repeating error for the operator. Revisit if a handback's text
    /// turns out to be an infrastructure error in practice.
    pub(in crate::orchestrator::core) fn hand_merge_back(
        &mut self,
        stage_id: &str,
        refusal: &str,
    ) -> Result<String, String> {
        let refusal = truncate_to_byte_limit(refusal, MAX_REFUSAL_BYTES);
        let stop = self.stop_resolvers_proven(stage_id);
        let stage = self.load_stage(stage_id).map_err(|e| format!("{e:#}"))?;
        let live_workers =
            crate::orchestrator::coherence::live_worker_sessions(&self.config.work_dir, &stage)
                .map(|live| !live.is_empty())
                .unwrap_or(true);
        let unproven = stop.as_ref().is_some_and(|stopped| !stopped.proven);
        if unproven || live_workers || stage.merge.handbacks >= MAX_MERGE_HANDBACKS {
            return self.hold_failed_handback(stage_id, &refusal, stop.as_ref());
        }
        self.requeue_handed_back(stage_id, refusal)
    }

    /// Whether the stage's branch holds a merge commit made after its
    /// completed commit, which proves a resolver merged. A lookup error is
    /// `false`: the caller fails closed to the resolver path, which keeps its
    /// budget.
    pub(in crate::orchestrator::core) fn resolver_merge_committed(&self, stage: &Stage) -> bool {
        let Some(commit) = stage.completed_commit.as_deref() else {
            return false;
        };
        match crate::git::merge::has_merge_commit_since(&self.config.repo_root, &stage.id, commit) {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(stage_id = %stage.id, %error, "Could not look for the resolver's merge commit");
                false
            }
        }
    }

    /// Requeue the stage with `refusal` for a fresh stage session, and drop
    /// the resolver's bookkeeping.
    fn requeue_handed_back(&mut self, stage_id: &str, refusal: String) -> Result<String, String> {
        let mut moved_on = false;
        let updated = self.update_stage(stage_id, |current| {
            if current.is_final_merged() || !awaits_merge(&current.status) {
                moved_on = true;
                anyhow::bail!("stage '{stage_id}' is {} on disk", current.status);
            }
            current.try_hand_back_merge(refusal.clone())
        });
        if moved_on {
            return Err(
                "the stage left its merge state meanwhile; it was not handed back".to_string(),
            );
        }
        if let Err(error) = updated {
            tracing::warn!(stage_id = %stage_id, error = %format!("{error:#}"), "Failed to save the handed back stage");
            return Err("the stage could not be saved; it was not handed back".to_string());
        }
        // The node follows the file; `mark_status` would log an illegal edge.
        if let Err(error) = self.graph.force_status(stage_id, StageStatus::Queued) {
            tracing::warn!(stage_id = %stage_id, %error, "Failed to mark the handed back stage Queued in the graph");
        }
        self.clear_merge_resolver_attempts(stage_id);
        clear_status_line();
        eprintln!(
            "Stage '{stage_id}' handed back: the resolver's content failed completion \
             verification, so a fresh stage session gets the failure"
        );
        Ok(
            "the stage was handed back to a stage session and this resolver was stopped"
                .to_string(),
        )
    }

    /// Keep the full `refusal` on the stage for a session the operator
    /// approves, then route the stage to human review.
    fn hold_failed_handback(
        &mut self,
        stage_id: &str,
        refusal: &str,
        stop: Option<&ResolverStop>,
    ) -> Result<String, String> {
        let stored = self.update_stage(stage_id, |current| {
            anyhow::ensure!(
                !current.is_final_merged() && awaits_merge(&current.status),
                "stage '{stage_id}' is {} on disk",
                current.status
            );
            current.merge.handback_refusal = Some(refusal.to_string());
            Ok(())
        });
        if let Err(error) = stored {
            tracing::warn!(stage_id = %stage_id, error = %format!("{error:#}"), "Failed to store the handback refusal");
        }
        let followup = match stop {
            Some(stopped) if !stopped.proven => stopped.note.clone(),
            _ => format!(
                "Run loom stage human-review {stage_id} --approve to give a fresh session the \
                 full failure, or {}",
                self.manual_merge_steps(stage_id)
            ),
        };
        let reason = format!("{}. {followup}", inline_safe(refusal));
        match self.route_merge_stage_to_review(stage_id, reason, None) {
            ReviewRoute::Routed => {
                self.clear_merge_resolver_attempts(stage_id);
                clear_status_line();
                eprintln!(
                    "Stage '{stage_id}' needs human review: the merge resolver's content failed \
                     completion verification"
                );
                Ok("the stage is held in needs-human-review".to_string())
            }
            ReviewRoute::StageMovedOn => Err(
                "the stage left its merge state meanwhile; it was not routed to human review"
                    .to_string(),
            ),
            ReviewRoute::NotSaved => {
                Err("the stage could not be saved; it was not routed to human review".to_string())
            }
        }
    }
}

#[cfg(test)]
#[path = "resolver_handback_tests.rs"]
mod tests;
