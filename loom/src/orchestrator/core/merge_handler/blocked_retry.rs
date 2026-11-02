//! The per-tick retry of a merge held by a typed [`MergeBlock`]. A blocked
//! merge needs no resolver: the operator clears the cause (an operation in
//! progress, a checkout of the target branch, an overlapping change), and the
//! next tick's `merge_stage` lands it.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::git::branch::resolve_target_branch;
use crate::git::cleanup::branch_exists_strict;
use crate::git::runner::run_git_bool;
use crate::git::{blocked_merge_inputs, branch_name_for_stage, get_branch_head, MergeBlock};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};
use crate::orchestrator::signals::find_live_merge_session_for_stage;
use crate::stage_service::merge_outcome::{apply_retry_failure, check_stage};

use super::landing::Landing;

/// The paths of the stage's current `UncommittedOverlap` block, which the
/// retry fingerprint watches: an ignored file among them is not in status.
fn overlap_paths(stage: &Stage) -> Vec<String> {
    match &stage.merge.block {
        Some(MergeBlock::UncommittedOverlap { paths }) => paths.clone(),
        _ => Vec::new(),
    }
}

/// The backup refs of stashes the stage holds, the block's own first: each
/// keeps changes of the operator that the checkout lacks, or a spare copy of
/// them after a refused reset, which a retry would stash again.
fn unrestored_backup_refs(stage: &Stage) -> Vec<&str> {
    let held = match &stage.merge.block {
        Some(
            MergeBlock::StashNotRestored { backup_ref }
            | MergeBlock::StashResetRefused { backup_ref },
        ) => Some(backup_ref),
        _ => None,
    };
    held.into_iter()
        .chain(&stage.merge.unrestored)
        .map(String::as_str)
        .collect()
}

/// Whether the operator's work still sits in a backup stash of `stage`: one
/// of its unrestored backup refs exists. A retry would stash the checkout
/// again, possibly half updated, under a new ref; the operator deleting the
/// ref after restoring resumes the retries.
fn holds_backup_stash(repo_root: &Path, stage: &Stage) -> bool {
    unrestored_backup_refs(stage).into_iter().any(|backup_ref| {
        backup_ref.starts_with("refs/loom/autostash/")
            && run_git_bool(&["rev-parse", "--verify", "--quiet", backup_ref], repo_root)
    })
}

/// A memo entry older than this is ignored: a safety net for inputs the
/// fingerprint cannot see (git config, attributes, permissions).
const MEMO_MAX_AGE: Duration = Duration::from_secs(10 * 60);

/// The least time between two retries of a merge that ended
/// `FastForwardRefused`; each attempt writes a merge commit object.
const REFUSED_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// Whether any `watched` path is a directory. A directory's modification
/// time does not move when a file deeper inside is removed, so the
/// fingerprint cannot watch it.
fn watches_directory(repo_root: &Path, watched: &[String]) -> bool {
    watched
        .iter()
        .any(|path| std::fs::symlink_metadata(repo_root.join(path)).is_ok_and(|meta| meta.is_dir()))
}

impl Orchestrator {
    /// Run `merge_stage` again for `stage`, which is `MergeBlocked` with a
    /// `merge_block`; see [`Self::retry_blocked_merge_at`].
    pub(super) fn retry_blocked_merge(&mut self, stage: &Stage) {
        self.retry_blocked_merge_at(stage, Instant::now());
    }

    /// Run `merge_stage` again for `stage` at time `now`. A merge that lands
    /// follows the verified-merge path and its worktree is cleaned up, unless
    /// a resolver still runs in it: its exit cleans up then. A conflict moves
    /// the stage to `MergeConflict`; a block that is still there leaves the
    /// stage for the next tick, and prints nothing. An error goes through
    /// [`Self::handle_retry_failure`].
    ///
    /// A retry is skipped while the inputs the block depends on are the ones
    /// the last blocked attempt saw, for at most ten minutes: an attempt
    /// writes git objects even when the target cannot advance. An attempt
    /// that watches a directory is never skipped, and a `FastForwardRefused`
    /// one is retried at most once a minute. A stage whose operator work
    /// still sits in a backup stash is not retried at all, and no stage is
    /// while the tick's target guard check left the target held: the retry
    /// resumes on the first tick after the operator accepts or restores it.
    pub(super) fn retry_blocked_merge_at(&mut self, stage: &Stage, now: Instant) {
        if self.target_held() {
            tracing::debug!(stage_id = %stage.id, "Target is held; merge retry paused");
            return;
        }
        let stage_id = stage.id.as_str();
        if holds_backup_stash(&self.config.repo_root, stage) {
            tracing::debug!(stage_id = %stage_id, "Operator changes are in a backup stash; retry paused");
            return;
        }
        if self.refused_retry_pending(stage_id, now) {
            tracing::debug!(stage_id = %stage_id, "Fast-forward refusal retried too recently; skipped");
            return;
        }
        let target = resolve_target_branch(&self.config.base_branch, &self.config.repo_root);
        let watched = overlap_paths(stage);
        let inputs = if watches_directory(&self.config.repo_root, &watched) {
            None
        } else {
            blocked_merge_inputs(&self.config.repo_root, stage_id, &target, &watched).ok()
        };
        if inputs.is_some_and(|inputs| self.memo_holds(stage_id, inputs, now)) {
            tracing::debug!(stage_id = %stage_id, "Merge block inputs unchanged; retry skipped");
            return;
        }
        let landing = self.land_stage_merge(stage_id, &target);
        self.remember_blocked_inputs(stage_id, inputs, &landing, now);
        match landing {
            Landing::Merged => self.finish_retried_merge(stage_id, &target),
            Landing::Unproven => tracing::warn!(
                stage_id = %stage_id,
                "Retried merge has no ancestry proof; merged stays false"
            ),
            Landing::Failed(error) => tracing::warn!(
                stage_id = %stage_id,
                %error,
                "Retry of a blocked merge failed; retrying next tick"
            ),
            Landing::Held | Landing::Conflict(_) | Landing::Blocked(_) => {}
        }
    }

    /// `merge_stage` failed for `stage`, which holds a typed block. A cause no
    /// tick can clear hands it to the operator: the block's remedy is stale.
    /// Any other failure says nothing about the block, which stays and keeps
    /// the promise that no resolver spawns for a merge with no conflict; the
    /// stage records the error so `loom status` says what the retry hits.
    pub(super) fn handle_retry_failure(
        &mut self,
        stage: &Stage,
        stage_id: &str,
        reason: String,
    ) -> Landing {
        if let Some(permanent) = self.permanent_merge_failure(stage, stage_id) {
            return self.hold_claimed_landing(stage, permanent);
        }
        let evidence: Vec<&str> = reason.lines().collect();
        let unchanged = stage
            .failure_info
            .as_ref()
            .is_some_and(|failure| failure.evidence == evidence);
        if !unchanged {
            let saved = self.update_stage(stage_id, |current| {
                check_stage(current, stage)?;
                apply_retry_failure(current, &reason)
            });
            if let Err(error) = saved {
                tracing::warn!(stage_id = %stage_id, %error, "Failed to save a merge retry failure");
            }
        }
        Landing::Failed(reason)
    }

    /// The remedy for a merge `merge_stage` refuses until an operator acts: the
    /// stage branch is gone, or it moved past the commit verification proved.
    /// A check that itself fails proves nothing, so it reads as no such cause.
    fn permanent_merge_failure(&self, stage: &Stage, stage_id: &str) -> Option<String> {
        let repo_root = &self.config.repo_root;
        let branch = branch_name_for_stage(stage_id);
        let steps = self.manual_merge_steps(stage_id);
        if !branch_exists_strict(&branch, repo_root).ok()? {
            return Some(format!(
                "branch {branch} is missing; restore it, then {steps}"
            ));
        }
        let tip = get_branch_head(&branch, repo_root).ok()?;
        let verified = stage.completed_commit.as_deref().unwrap_or("no commit");
        (verified != tip).then(|| {
            format!("branch {branch} moved from {verified} to {tip} after verification; {steps}")
        })
    }

    /// Drop the retry memo of every stage that is no longer `MergeBlocked`
    /// with a typed block, as read from its file (a read error keeps the
    /// memo): a stage that left that state by other means (an
    /// operator re-queue, a conflict) must not carry its entry into a later
    /// block with identical inputs, which would skip that block's first retry.
    pub(super) fn prune_retry_memos(&mut self) {
        let remembered: Vec<String> = self
            .blocked_merge_inputs
            .keys()
            .chain(self.refused_merge_attempts.keys())
            .cloned()
            .collect();
        for stage_id in remembered {
            let left_blocked = self.load_stage(&stage_id).is_ok_and(|stage| {
                stage.status != StageStatus::MergeBlocked || stage.merge.block.is_none()
            });
            if left_blocked {
                self.blocked_merge_inputs.remove(&stage_id);
                self.refused_merge_attempts.remove(&stage_id);
            }
        }
    }

    /// Whether the memo holds `inputs` for the stage, recorded under
    /// [`MEMO_MAX_AGE`] before `now`.
    fn memo_holds(&self, stage_id: &str, inputs: u64, now: Instant) -> bool {
        self.blocked_merge_inputs
            .get(stage_id)
            .is_some_and(|(seen, at)| {
                *seen == inputs && now.saturating_duration_since(*at) < MEMO_MAX_AGE
            })
    }

    /// Whether the stage's last attempt ended `FastForwardRefused` less than
    /// [`REFUSED_RETRY_INTERVAL`] before `now`.
    fn refused_retry_pending(&self, stage_id: &str, now: Instant) -> bool {
        self.refused_merge_attempts
            .get(stage_id)
            .is_some_and(|at| now.saturating_duration_since(*at) < REFUSED_RETRY_INTERVAL)
    }

    /// Keep `inputs`, computed before the attempt, when it ended in a block
    /// that only a change of those inputs can clear (a blocked attempt
    /// changes nothing in the main checkout); `FastForwardRefused` is timed
    /// instead, since a transient lock leaves no trace in the inputs. Every
    /// other outcome drops both entries; `TargetHeld` is never memoised, so
    /// the retry runs on the first tick after the hold clears.
    fn remember_blocked_inputs(
        &mut self,
        stage_id: &str,
        inputs: Option<u64>,
        landing: &Landing,
        now: Instant,
    ) {
        self.blocked_merge_inputs.remove(stage_id);
        self.refused_merge_attempts.remove(stage_id);
        match (inputs, landing) {
            (
                Some(inputs),
                Landing::Blocked(
                    MergeBlock::UncommittedOverlap { .. }
                    | MergeBlock::TargetCheckedOutElsewhere { .. }
                    | MergeBlock::OperatorOperation { .. },
                ),
            ) => {
                self.blocked_merge_inputs
                    .insert(stage_id.to_string(), (inputs, now));
            }
            (_, Landing::Blocked(MergeBlock::FastForwardRefused { .. })) => {
                self.refused_merge_attempts
                    .insert(stage_id.to_string(), now);
            }
            _ => {}
        }
    }

    fn finish_retried_merge(&mut self, stage_id: &str, target: &str) {
        clear_status_line();
        if self.resolver_may_be_live(stage_id) {
            eprintln!(
                "Stage '{stage_id}' merged after its block cleared; its worktree is removed \
                 once the merge resolver exits"
            );
            return;
        }
        self.cleanup_resolved_merge(stage_id, target);
    }

    /// Whether a merge resolver may still run in the stage worktree: one is
    /// tracked, a signal names a live one, or that could not be told.
    fn resolver_may_be_live(&self, stage_id: &str) -> bool {
        if self.active_sessions.contains_key(stage_id) {
            return true;
        }
        match find_live_merge_session_for_stage(stage_id, &self.config.work_dir) {
            Ok(live) => live.is_some(),
            Err(error) => {
                tracing::warn!(stage_id = %stage_id, %error, "Cannot tell whether a merge resolver runs");
                true
            }
        }
    }
}

#[cfg(test)]
#[path = "blocked_retry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "blocked_retry_failure_tests.rs"]
mod failure_tests;
