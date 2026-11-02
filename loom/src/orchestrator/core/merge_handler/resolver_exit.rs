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
use crate::verify::completion::OutputStream;

use super::landing::Landing;
use super::review_route::awaits_merge;

/// Where the daemon's completion checks of a resolved merge report, each
/// failing command included: a daemon has no terminal to print them on.
pub(in crate::orchestrator::core) fn trace_verification_output(stream: OutputStream, line: &str) {
    match stream {
        OutputStream::Stdout => tracing::info!("{line}"),
        OutputStream::Stderr => tracing::warn!("{line}"),
    }
}

impl Orchestrator {
    /// Drop the tracked entry of `stage_id` when it is the resolver
    /// `session_id`. After a handback the entry may be the fresh stage session.
    fn forget_tracked_resolver(&mut self, stage_id: &str, session_id: &str) {
        if self
            .active_sessions
            .get(stage_id)
            .is_some_and(|tracked| tracked.id == session_id)
        {
            self.active_sessions.remove(stage_id);
        }
    }

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
        self.forget_tracked_resolver(stage_id, session_id);
        self.fold_back_resolver_permissions(stage_id);

        let mut stage = self.load_stage(stage_id)?;
        let target = resolve_target_branch(&self.config.base_branch, &self.config.repo_root);
        // A final stage is not re-verified: only the cleanup is left to do.
        if stage.is_final_merged() || (stage.merged && self.merged_flag_is_proven(&stage, &target))
        {
            self.cleanup_resolved_merge(stage_id, &target);
            self.clear_merge_resolver_attempts(stage_id);
            clear_status_line();
            eprintln!("Stage '{stage_id}' merge completed successfully");
            return Ok(());
        }
        if stage.merged {
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

    /// Whether git proves `stage`, flagged merged but not final (a merged flag
    /// on a stage that is not `Completed`), is in `target`. Such a flag is not
    /// trusted blindly; only knowledge stages (no branch by design) are.
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

    /// Stop any check the resolver left running, check the worktree it left,
    /// land the merge, and clean up. Content that fails completion verification
    /// is handed back to a stage session ([`Self::hand_merge_back`]) once a
    /// merge commit is proved. A worktree that is not ready, content that fails with no merge commit, or
    /// a merge that does not land, leaves the stage as it is: the spawn loop
    /// gives it another counted resolver, or routes it to human review once
    /// the budget is spent.
    fn land_resolved_worktree(
        &mut self,
        stage_id: &str,
        target: &str,
        completed_commit: Option<&str>,
    ) {
        if let Err(reason) = self.stop_running_check(stage_id) {
            self.report_unresolved_merge(stage_id, &reason);
            return;
        }
        if let Err(reason) =
            check_resolved_worktree(&self.config.repo_root, stage_id, completed_commit)
        {
            self.report_unresolved_merge(stage_id, &reason);
            return;
        }
        let stage = match self.load_stage(stage_id) {
            Ok(stage) => stage,
            Err(error) => {
                self.report_unresolved_merge(stage_id, &format!("{error:#}"));
                return;
            }
        };
        if let Err(error) = crate::verify::completion::resolved::verify_resolved_completion(
            &stage,
            &self.config.work_dir,
            trace_verification_output,
        ) {
            self.settle_failed_exit_verification(&stage, &error);
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

    /// The resolver exited with content that failed verification. With a
    /// merge commit proved it goes back to a stage session; without one the
    /// stage stays, so the spawn loop gives it another counted resolver.
    fn settle_failed_exit_verification(&mut self, stage: &Stage, error: &anyhow::Error) {
        let stage_id = stage.id.as_str();
        let refusal = format!("resolver content failed completion verification: {error:#}");
        if !self.resolver_merge_committed(stage) {
            self.report_unresolved_merge(stage_id, &refusal);
            return;
        }
        match self.hand_merge_back(stage_id, &refusal) {
            Ok(note) => {
                clear_status_line();
                eprintln!("Stage '{stage_id}': {note}");
            }
            Err(why) => {
                tracing::warn!(stage_id = %stage_id, %why, "Failed verification was not handed back");
            }
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
