//! What the daemon does when a merge resolver's process ends. The resolver
//! merged the target into the stage branch in the stage worktree; the daemon
//! checks that worktree, merges the stage into the target with `merge_stage`,
//! and only then removes the worktree: the resolver has exited, so nothing
//! runs in it any more.

use anyhow::Result;

use crate::git::branch::resolve_target_branch;
use crate::git::merge::{check_resolved_worktree, verify_merge_succeeded};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};
use crate::orchestrator::signals::remove_signal;

use super::landing::Landing;
use super::review_route::awaits_merge;

impl Orchestrator {
    pub(in crate::orchestrator::core) fn handle_merge_session_completed(
        &mut self,
        session_id: &str,
        stage_id: &str,
    ) -> Result<()> {
        clear_status_line();
        eprintln!("Merge session '{session_id}' completed for stage '{stage_id}'");
        if let Err(e) = remove_signal(session_id, &self.config.work_dir) {
            eprintln!("Warning: Failed to remove merge signal: {e}");
        }
        self.active_sessions.remove(stage_id);
        self.fold_back_resolver_permissions(stage_id);

        let mut stage = self.load_stage(stage_id)?;
        let target = resolve_target_branch(&self.config.base_branch, &self.config.repo_root);
        if stage.merged {
            if self.merged_flag_is_proven(&stage, &target) {
                self.cleanup_resolved_merge(stage_id, &target);
                self.clear_merge_resolver_attempts(stage_id);
                clear_status_line();
                eprintln!("Stage '{stage_id}' merge completed successfully");
                return Ok(());
            }
            stage = self.revert_unproven_merge(stage_id, stage);
        }
        match stage.status {
            StageStatus::MergeBlocked if stage.merge.block.is_some() => {
                // The per-tick retry lands the merge and cleans up, the
                // resolver being gone.
                tracing::info!(stage_id = %stage_id, "Resolver exited; the blocked merge is retried each tick");
            }
            ref status if awaits_merge(status) => {
                self.land_resolved_worktree(stage_id, &target, stage.completed_commit.as_deref());
            }
            ref status => tracing::info!(
                stage_id = %stage_id,
                %status,
                "Merge resolver exited after the stage left its merge state"
            ),
        }
        Ok(())
    }

    /// Record the approvals the resolver collected in `.worktrees/<id>` before
    /// any cleanup path removes it. Best effort: the inbox sweep that retires
    /// the session finds the worktree gone and skips its own fold-back.
    fn fold_back_resolver_permissions(&self, stage_id: &str) {
        let repo_root = &self.config.repo_root;
        let worktree = repo_root.join(".worktrees").join(stage_id);
        if !worktree.exists() {
            return;
        }
        if let Err(error) = crate::fs::permissions::sync_worktree_permissions(&worktree, repo_root)
        {
            tracing::warn!(stage_id = %stage_id, error = %format!("{error:#}"), "Permission fold-back from the merge resolver's worktree failed");
        }
    }

    /// Whether git proves `stage`, flagged merged, is in `target`. A flag is
    /// not trusted blindly; only knowledge stages (no branch by design) are.
    fn merged_flag_is_proven(&self, stage: &Stage, target: &str) -> bool {
        if stage.stage_type == StageType::Knowledge {
            return true;
        }
        stage.completed_commit.as_deref().is_some_and(|commit| {
            verify_merge_succeeded(commit, target, &self.config.repo_root).unwrap_or(false)
        })
    }

    /// Reset `merged` on a stage whose flag git does not back, and say so.
    fn revert_unproven_merge(&mut self, stage_id: &str, stage: Stage) -> Stage {
        tracing::error!(
            stage_id = %stage_id,
            "Merge session ended with merged=true but ancestry verification failed; \
             resetting merged to false"
        );
        clear_status_line();
        eprintln!(
            "Stage '{stage_id}' was marked merged but git ancestry does not show it; merged \
             reset to false. Run: loom stage merge {stage_id}"
        );
        self.update_stage(stage_id, |current| {
            current.merged = false;
            Ok(())
        })
        .unwrap_or_else(|error| {
            tracing::warn!(stage_id = %stage_id, %error, "Failed to reset merged after failed verification");
            stage
        })
    }

    /// Check the worktree the resolver left, land the merge, and clean up.
    /// A worktree that is not ready, or a merge that does not land, leaves the
    /// stage as it is: the spawn loop gives it another counted resolver, or
    /// routes it to human review once the budget is spent.
    fn land_resolved_worktree(
        &mut self,
        stage_id: &str,
        target: &str,
        completed_commit: Option<&str>,
    ) {
        if let Err(reason) =
            check_resolved_worktree(&self.config.repo_root, stage_id, completed_commit)
        {
            self.report_unresolved_merge(stage_id, &reason);
            return;
        }
        let verification = self.load_stage(stage_id).and_then(|stage| {
            crate::commands::stage::complete::verify_resolved_completion(
                &stage,
                &self.config.work_dir,
            )
        });
        if let Err(error) = verification {
            let reason = format!(
                "resolver content failed completion verification: {error:#}; fix the failure \
                 and run `loom stage complete {stage_id}` before landing"
            );
            self.route_merge_stage_to_review(stage_id, reason.clone(), None);
            eprintln!("Stage '{stage_id}': {reason}");
            return;
        }
        match self.land_stage_merge(stage_id, target) {
            Landing::Merged => self.cleanup_resolved_merge(stage_id, target),
            Landing::Unproven => {
                self.report_unresolved_merge(stage_id, "no ancestry proof that the stage landed");
            }
            Landing::Failed(error) => self.report_unresolved_merge(stage_id, &error),
            // Each of these already told the operator what happened.
            Landing::Held | Landing::Conflict(_) | Landing::Blocked(_) => {}
        }
    }

    fn report_unresolved_merge(&self, stage_id: &str, reason: &str) {
        clear_status_line();
        eprintln!(
            "Stage '{stage_id}': the merge resolver ended without a mergeable result: {reason}"
        );
        eprintln!(
            "  Loom spawns another resolver in .worktrees/{stage_id}, or routes the stage to \
             human review once the resolver budget is spent."
        );
    }
}

#[cfg(test)]
#[path = "resolver_exit_tests.rs"]
mod tests;
