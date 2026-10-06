//! Landing a stage's merge from the daemon after a resolver worked on it or a
//! block cleared: `merge_stage` (control-path gate included) and recording
//! what came back. The `--resolved` handler, the resolver-exit path and the per-tick
//! retry of a blocked merge all land through [`Orchestrator::land_stage_merge`].
//!
//! PHANTOM-MERGE INVARIANT: `merged = true` is written only by
//! [`Orchestrator::finalize_merge_resolution`], after git ancestry proves the
//! stage's commit is in the target.

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::CleanupConfig;
use crate::git::merge::{
    merge_stage, verify_merge_succeeded, MergeBlock, MergeGate, MergeResult, StashReapply,
};
use crate::git::signing::CommitTreeError;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};
use crate::orchestrator::merge_lifecycle::finish_verified_merge;

use super::report_deferred_cleanup;
use super::review_route::ReviewRoute;

/// What landing a stage's merge came to.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::orchestrator::core) enum Landing {
    /// The merge landed and ancestry proved it: the stage is `Completed` and
    /// `merged`.
    Merged,
    /// The stage was routed to human review: the control-path gate held the
    /// branch, or loom's own merge commit could not be signed.
    Held,
    /// The merge conflicts in these paths; the stage is `MergeConflict`.
    Conflict(Vec<String>),
    /// The merge was not advanced; the stage is `MergeBlocked` with the block
    /// recorded.
    Blocked(MergeBlock),
    /// `merge_stage` reported success but ancestry gave no proof; `merged`
    /// stays false.
    Unproven,
    /// The merge could not be attempted; the stage is unchanged.
    Failed(String),
}

/// Print where the operator's stashed changes are, when a merge stashed them.
/// Changes that could not be reapplied are a warning: the merge did land.
pub(in crate::orchestrator::core) fn report_stash(stage_id: &str, stash: Option<&StashReapply>) {
    let Some(stash) = stash else { return };
    clear_status_line();
    if stash.restored {
        tracing::info!(stage_id = %stage_id, backup_ref = %stash.backup_ref, "Main checkout changes stashed around a merge");
        eprintln!("Stage '{stage_id}': {}", stash.notice());
    } else {
        tracing::warn!(stage_id = %stage_id, backup_ref = %stash.backup_ref, "Stashed changes were not reapplied after a merge");
        eprintln!("WARNING: stage '{stage_id}': {}", stash.notice());
    }
}

impl Orchestrator {
    /// Report the stash outcome of a merge that landed and record it on the
    /// stage, so `loom status` still shows unrestored changes after the
    /// terminal output is gone.
    pub(in crate::orchestrator::core) fn note_merge_stash(
        &mut self,
        stage_id: &str,
        stash: Option<StashReapply>,
    ) {
        report_stash(stage_id, stash.as_ref());
        let Some(stash) = stash else { return };
        let saved = self.update_stage(stage_id, |current| {
            current.record_merge_stash(stash);
            Ok(())
        });
        if let Err(error) = saved {
            tracing::warn!(stage_id = %stage_id, %error, "Failed to record the merge stash outcome");
        }
    }

    /// Merge `stage_id` into `target` with `merge_stage`, whose control-path
    /// gate reads the commits it merges, and leave the stage in the status
    /// the result calls for.
    pub(in crate::orchestrator::core) fn land_stage_merge(
        &mut self,
        stage_id: &str,
        target: &str,
    ) -> Landing {
        let mut stage = match self.load_stage(stage_id) {
            Ok(stage) => stage,
            Err(error) => return Landing::Failed(format!("{error:#}")),
        };
        let repo_root = self.config.repo_root.clone();
        let work_dir = self.config.work_dir.clone();
        match merge_stage(stage_id, target, &repo_root, &work_dir, MergeGate::Enforce) {
            Ok(MergeResult::Success { stash, .. }) => {
                self.note_merge_stash(stage_id, stash);
                self.verify_landing(&mut stage, stage_id, target)
            }
            Ok(MergeResult::AlreadyUpToDate) => self.verify_landing(&mut stage, stage_id, target),
            Ok(MergeResult::Conflict { conflicting_files }) => {
                self.record_merge_conflict(stage_id, conflicting_files.len());
                Landing::Conflict(conflicting_files)
            }
            Ok(MergeResult::Blocked(block)) => {
                self.record_merge_block(stage_id, block.clone());
                Landing::Blocked(block)
            }
            Ok(MergeResult::Held { reason }) => {
                self.route_to_human_review(stage_id, reason, None);
                Landing::Held
            }
            Err(error) => match signing_failure(&error) {
                Some(failure) => {
                    let reason = merge_signing_reason(stage_id, &failure.detail);
                    self.route_to_human_review(stage_id, reason, None);
                    Landing::Held
                }
                None => Landing::Failed(format!("{error:#}")),
            },
        }
    }

    /// Hold `stage_id`'s merge for the operator after the daemon's relayed
    /// merge commit failed to sign: stop any resolver (it cannot fix signing),
    /// then route the stage to `NeedsHumanReview` with the remedy. Returns the
    /// outcome text for the refused request.
    pub(in crate::orchestrator::core) fn hold_merge_for_signing(
        &mut self,
        stage_id: &str,
        detail: &str,
    ) -> String {
        let mut reason = merge_signing_reason(stage_id, detail);
        if let Some(stopped) = self.stop_gated_resolvers(stage_id) {
            clear_status_line();
            eprintln!("Stage '{stage_id}': merge commit signing failed; {stopped}");
            reason = format!("{reason}. {stopped}");
        }
        match self.route_merge_stage_to_review(stage_id, reason, None) {
            ReviewRoute::Routed => "held for the operator in needs-human-review".to_string(),
            ReviewRoute::StageMovedOn => {
                "the stage left its merge state meanwhile; it was not routed to human review"
                    .to_string()
            }
            ReviewRoute::NotSaved => {
                "the stage could not be saved; it was not routed to human review".to_string()
            }
        }
    }

    fn verify_landing(&mut self, stage: &mut Stage, stage_id: &str, target: &str) -> Landing {
        let message = "merge verified and marked as complete";
        if self.finalize_merge_resolution(stage, stage_id, target, message) {
            Landing::Merged
        } else {
            Landing::Unproven
        }
    }

    /// Finalize a landed merge: write `merged=true`, transition to Completed,
    /// clear the merge block, update the graph and the resolver attempt
    /// counter. The resolver's session tracking and signal are left to the
    /// caller: the resolver may still be running.
    ///
    /// PHANTOM-MERGE INVARIANT: this is a daemon-side automated path, so it MUST
    /// NOT write `merged=true` without git ancestry proof. Before finalizing it
    /// derives `completed_commit` from the stage branch HEAD when missing and
    /// requires `verify_merge_succeeded(commit, merge_point)` to return
    /// `Ok(true)`. If that proof is unavailable the stage is left unchanged and
    /// the function returns `false`. Mirrors `verify_and_finalize_merge`.
    ///
    /// Returns `true` only when the merge was ancestry-verified and finalized.
    pub(in crate::orchestrator::core) fn finalize_merge_resolution(
        &mut self,
        stage: &mut Stage,
        stage_id: &str,
        merge_point: &str,
        log_message: &str,
    ) -> bool {
        let Some(completed_commit) = self.provable_commit(stage, stage_id, merge_point) else {
            return false;
        };
        let updated = self.update_stage(stage_id, |current| {
            apply_finalized(current, &completed_commit)
        });
        match updated {
            Ok(updated) => *stage = updated,
            Err(error) => {
                tracing::warn!(stage_id = %stage_id, %error, "Failed to persist merge resolution");
                return false;
            }
        }
        self.graph.set_node_merged(stage_id, true);
        if let Err(e) = self.graph.mark_completed(stage_id) {
            eprintln!("Warning: Failed to mark stage as completed in graph: {e}");
        }
        self.clear_merge_resolver_attempts(stage_id);
        clear_status_line();
        eprintln!("Stage '{stage_id}' {log_message}");
        true
    }

    /// The stage's commit once git ancestry proves it is in `merge_point`.
    /// `completed_commit` is derived from the stage branch HEAD when missing;
    /// with neither, or without the proof, `None`.
    fn provable_commit(
        &self,
        stage: &mut Stage,
        stage_id: &str,
        merge_point: &str,
    ) -> Option<String> {
        if stage.completed_commit.is_none() {
            let branch = branch_name_for_stage(stage_id);
            match crate::git::get_branch_head(&branch, &self.config.repo_root) {
                Ok(head) => stage.completed_commit = Some(head),
                Err(_) => {
                    tracing::error!(
                        stage_id = %stage_id,
                        "Cannot finalize merge: no completed_commit and branch HEAD \
                         unavailable; refusing to write merged=true (phantom-merge prevention)"
                    );
                    return None;
                }
            }
        }
        let commit = stage.completed_commit.clone()?;
        match verify_merge_succeeded(&commit, merge_point, &self.config.repo_root) {
            Ok(true) => Some(commit),
            other => {
                tracing::error!(
                    stage_id = %stage_id,
                    %commit,
                    target = %merge_point,
                    verified = ?other,
                    "Refusing to finalize merge: ancestry verification did not pass \
                     (phantom-merge prevention)"
                );
                None
            }
        }
    }

    /// Move `stage_id` to `MergeConflict` after a merge found `conflicts`
    /// paths in conflict; the spawn loop gives it a counted resolver.
    pub(in crate::orchestrator::core) fn record_merge_conflict(
        &mut self,
        stage_id: &str,
        conflicts: usize,
    ) {
        // The branch head now is the stage's own work, before any resolver
        // touches the branch; `check_resolved_worktree` holds it to that.
        let head =
            crate::git::get_branch_head(&branch_name_for_stage(stage_id), &self.config.repo_root)
                .ok();
        let saved = self.update_stage(stage_id, |current| {
            ensure_unmerged(current)?;
            current.record_completed_commit_if_missing(head.as_deref());
            current.enter_merge_conflict();
            Ok(())
        });
        if let Err(error) = saved {
            tracing::warn!(stage_id = %stage_id, %error, "Failed to save the merge conflict");
            return;
        }
        if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeConflict) {
            eprintln!("Warning: Failed to mark stage as merge conflict in graph: {e}");
        }
        clear_status_line();
        eprintln!(
            "Stage '{stage_id}' has {conflicts} conflict(s); a resolver will work in \
             .worktrees/{stage_id}"
        );
    }

    /// Record `block` as why `stage_id`'s merge did not advance. Returns
    /// whether the block is new: an unchanged block writes nothing and prints
    /// nothing, so the per-tick retry stays quiet.
    pub(in crate::orchestrator::core) fn record_merge_block(
        &mut self,
        stage_id: &str,
        block: MergeBlock,
    ) -> bool {
        if let Ok(current) = self.load_stage(stage_id) {
            if current.status == StageStatus::MergeBlocked
                && current.merge.block.as_ref() == Some(&block)
            {
                tracing::debug!(stage_id = %stage_id, %block, "Merge still blocked");
                return false;
            }
        }
        let shown = block.to_string();
        let saved = self.update_stage(stage_id, |current| {
            ensure_unmerged(current)?;
            current.block_merge(block);
            Ok(())
        });
        if let Err(error) = saved {
            tracing::warn!(stage_id = %stage_id, %error, "Failed to save the merge block");
            return false;
        }
        let _ = self.graph.mark_status(stage_id, StageStatus::MergeBlocked);
        clear_status_line();
        eprintln!("Stage '{stage_id}' merge is blocked: {shown}");
        eprintln!("  Loom retries the merge every tick.");
        true
    }

    /// Post-merge base reconcile and worktree cleanup of a merged stage, its
    /// outcome reported. Call only once the merge is verified and no resolver
    /// is running in the worktree.
    pub(in crate::orchestrator::core) fn cleanup_resolved_merge(
        &self,
        stage_id: &str,
        target: &str,
    ) {
        let outcome = finish_verified_merge(
            stage_id,
            &self.config.repo_root,
            &self.config.work_dir,
            target,
            &CleanupConfig::quiet(),
        );
        report_deferred_cleanup(stage_id, &outcome);
    }
}

/// The signing failure of loom's own merge commit in `error`'s chain, found by
/// downcast: a `CommitTreeError` that is not a signing failure is not one.
pub(super) fn signing_failure(error: &anyhow::Error) -> Option<&CommitTreeError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<CommitTreeError>())
        .filter(|failure| failure.signing)
}

/// The review reason of a merge held because its commit could not be signed.
pub(super) fn merge_signing_reason(stage_id: &str, detail: &str) -> String {
    format!(
        "merge commit signing failed: {detail}; fix signing (gpg-agent passphrase cache, GUI \
         pinentry or ssh-agent key), then loom stage human-review {stage_id} --approve"
    )
}

/// A merge result may only be recorded on a stage that is not merged yet.
fn ensure_unmerged(stage: &Stage) -> anyhow::Result<()> {
    if stage.merged {
        anyhow::bail!("stage '{}' is already merged", stage.id);
    }
    Ok(())
}

/// Mark `current` merged with `completed_commit`, `Completed`, and clear its
/// merge block. Refuses a stage that left the merge states meanwhile or whose
/// commit changed under the proof.
fn apply_finalized(current: &mut Stage, completed_commit: &str) -> anyhow::Result<()> {
    if !matches!(
        current.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked | StageStatus::Completed
    ) {
        anyhow::bail!(
            "stage status changed to {} during merge verification",
            current.status
        );
    }
    match current.completed_commit.as_deref() {
        Some(fresh) if fresh != completed_commit => {
            anyhow::bail!("completed_commit changed during merge verification")
        }
        None => current.completed_commit = Some(completed_commit.to_string()),
        Some(_) => {}
    }
    current.merged = true;
    current.merge_conflict = false;
    current.clear_merge_block();
    if current.status != StageStatus::Completed {
        if let Err(error) = current.try_transition(StageStatus::Completed) {
            current.force_status_with_reason(
                StageStatus::Completed,
                &format!("merge resolved but transition was illegal: {error}"),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "landing_tests.rs"]
mod tests;
