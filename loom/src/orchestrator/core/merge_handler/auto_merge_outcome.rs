//! The two halves of `try_auto_merge` around the merge itself: the guards
//! before it and the handling of its outcome after it.

use anyhow::Result;

use crate::git::branch::{branch_name_for_stage, commits_ahead_of};
use crate::git::MergeBlock;
use crate::models::stage::Stage;
use crate::orchestrator::auto_merge::AutoMergeResult;
use crate::orchestrator::core::{clear_status_line, Orchestrator};

use crate::stage_service::merge_outcome::{classify_failure, MergeDisposition, MergeFailure};

impl Orchestrator {
    /// Returns true when a guard stops the auto-merge of `stage_id` into
    /// `target`: the target guard holds the target, the merge gate holds the
    /// branch, or the branch has no commit beyond the target.
    ///
    /// A held target comes first, from a fresh guard check: the stage stays
    /// `MergeBlocked` with a `TargetHeld` block, before the ancestry finalize
    /// of a stage with no worktree or the zero-commit route below could take
    /// a target an agent moved as the stage's merge. The check never answers
    /// clear for a move it could not judge: with the merge lock taken it
    /// judges the target without the lock, and an error holds the target.
    ///
    /// Phantom-merge guard: an existing branch with zero commits beyond the
    /// target would "merge" as a no-op without proving new work landed. Such a
    /// stage goes to human review, so dependents do not unblock. A missing
    /// branch skips the branch guards: the merge attempt reports it with its
    /// own recovery handling.
    pub(super) fn auto_merge_precheck_blocks(&mut self, stage_id: &str, target: &str) -> bool {
        if let Some(hold) = self.check_target_guard() {
            let block = MergeBlock::TargetHeld {
                target: target.into(),
                accepted: hold.accepted,
                observed: hold.observed,
            };
            self.record_merge_block(stage_id, block);
            return true;
        }
        let branch = branch_name_for_stage(stage_id);
        let exists =
            crate::git::branch::branch_exists(&branch, &self.config.repo_root).unwrap_or(false);
        if !exists {
            return false;
        }
        if self.merge_gate_blocks(stage_id, &branch, target) {
            return true;
        }
        match commits_ahead_of(&branch, target, &self.config.repo_root) {
            Ok(0) => {
                tracing::error!(
                    stage_id = %stage_id,
                    %branch,
                    %target,
                    "Stage branch has zero commits beyond target; routing to human review"
                );
                let reason = Stage::zero_commit_reason(stage_id, target);
                self.route_to_human_review(stage_id, reason, None);
                true
            }
            Ok(_) => false,
            Err(error) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    %branch,
                    %error,
                    "commits_ahead_of probe failed; proceeding with merge attempt"
                );
                false
            }
        }
    }

    /// Act on the result of `attempt_auto_merge`; returns whether the stage
    /// merged. A conflict moves the stage to `MergeConflict` and spawns
    /// nothing here: the spawn loop gives it a counted resolver. A block is
    /// recorded on the stage and retried every tick. A control-path hold goes
    /// to human review.
    pub(super) fn apply_auto_merge_outcome(
        &mut self,
        stage: &mut Stage,
        stage_id: &str,
        target: &str,
        outcome: Result<AutoMergeResult>,
    ) -> bool {
        match outcome.map(MergeDisposition::from) {
            Ok(MergeDisposition::Landed {
                files: files_changed,
                insertions,
                deletions,
                stash,
            }) => {
                self.note_merge_stash(stage_id, stash);
                let summary = format!("merged: {files_changed} files, +{insertions} -{deletions}");
                self.finalize_auto_merge(stage, stage_id, target, &summary)
            }
            Ok(MergeDisposition::AlreadyLanded) => {
                self.finalize_auto_merge(stage, stage_id, target, "already up to date")
            }
            Ok(MergeDisposition::Conflict(conflicting_files)) => {
                self.record_merge_conflict_claimed(stage_id, conflicting_files.len(), Some(stage));
                false
            }
            Ok(MergeDisposition::Blocked(block)) => {
                self.record_merge_block_claimed(stage_id, block, Some(stage));
                false
            }
            Ok(MergeDisposition::Held(reason)) => {
                self.route_claimed_to_review(stage, reason);
                false
            }
            // Nothing to merge: the stage may have been created without a worktree.
            Ok(MergeDisposition::MissingWorktree) => {
                self.verify_and_finalize_merge(stage, stage_id, target)
            }
            Ok(MergeDisposition::MissingBranch) => {
                self.persist_merge_blocked(stage, stage_id, "stage branch is missing");
                false
            }
            Err(error) => {
                self.settle_auto_merge_error(stage, stage_id, &error);
                false
            }
        }
    }

    /// Record a failed auto-merge attempt. A signing failure is the operator's
    /// to fix: a resolver cannot, and a MergeBlocked stage would sign again on
    /// the next spawn pass, so it goes to human review. Any other error leaves
    /// the stage MergeBlocked with the error recorded, so status shows it and
    /// `loom stage merge` can retry.
    fn settle_auto_merge_error(
        &mut self,
        stage: &mut Stage,
        stage_id: &str,
        error: &anyhow::Error,
    ) {
        clear_status_line();
        tracing::error!(stage_id = %stage_id, %error, "Auto-merge failed");
        match classify_failure(stage_id, error) {
            MergeFailure::Signing(reason) => {
                self.route_claimed_to_review(stage, reason);
            }
            MergeFailure::Infrastructure(reason) => {
                self.persist_merge_blocked(stage, stage_id, &reason)
            }
        }
    }
}
