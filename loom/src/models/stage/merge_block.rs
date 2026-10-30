//! The typed reason a stage's merge was not advanced (`Stage::merge`)
//! and the transitions that set and clear it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{Stage, StageStatus};
use crate::git::{MergeBlock, StashReapply};
use crate::models::failure::{FailureInfo, FailureType};

/// A stage's merge state, flattened into the stage file so `merge_block` and
/// `merge_stash` stay top-level keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeRecord {
    /// Why the target branch was not advanced; set while `MergeBlocked`.
    #[serde(
        rename = "merge_block",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub block: Option<MergeBlock>,
    /// What the merge did with the operator's stashed changes in the main
    /// checkout; never cleared, it is the stage's merge note.
    #[serde(
        rename = "merge_stash",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub stash: Option<StashReapply>,
    /// Every backup ref whose stashed changes a merge could not restore, in
    /// the order recorded. Never shrinks: the status collector reports the
    /// ones that still exist, and the operator deleting a ref closes it.
    #[serde(
        rename = "merge_unrestored_stashes",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub unrestored: Vec<String>,
    /// When the stage entered `MergeConflict`; set while it is there, and
    /// moved into the conflict's `issue_history` entry when it leaves.
    #[serde(
        rename = "merge_conflict_since",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub conflict_since: Option<DateTime<Utc>>,
    /// How many times a resolver's committed content failed completion
    /// verification and the stage went back to a stage session; caps the
    /// loop. Reset by `loom stage human-review --approve` and `loom stage
    /// reset`.
    #[serde(rename = "merge_handbacks", default, skip_serializing_if = "is_zero")]
    pub handbacks: u32,
    /// The verification failure of the latest handback. It is cleared when a
    /// completion succeeds or the stage is reset, and replaced by the next
    /// handback.
    #[serde(
        rename = "merge_handback_refusal",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub handback_refusal: Option<String>,
}

fn is_zero(count: &u32) -> bool {
    *count == 0
}

impl Stage {
    /// Whether the stage is final: `Completed` and merged, by loom or by the
    /// operator's `loom stage complete --force-unsafe --assume-merged`. Loom
    /// never re-verifies, reverts or moves a final stage, whatever happens to
    /// the target branch afterwards (an owner decision): recovery, merge and
    /// attribution paths all leave it as it is.
    pub fn is_final_merged(&self) -> bool {
        self.status == StageStatus::Completed && (self.merged || self.merge_assumed)
    }

    /// Record `block` as the reason the merge did not advance and move the
    /// stage to `MergeBlocked`, with the sentence in `failure_info` so
    /// `loom status` shows it. The daemon retries such a stage every tick:
    /// re-recording the same block keeps its failure as recorded, and a
    /// different one archives the failure it replaces.
    pub fn block_merge(&mut self, block: MergeBlock) {
        let sentence = block.to_string();
        self.replace_failure(FailureInfo {
            failure_type: FailureType::InfrastructureError,
            detected_at: Utc::now(),
            evidence: vec![sentence.clone()],
        });
        if let MergeBlock::StashNotRestored { backup_ref } = &block {
            self.record_merge_stash(StashReapply {
                backup_ref: backup_ref.clone(),
                restored: false,
            });
        }
        self.merge.block = Some(block);
        if self.status != StageStatus::MergeBlocked && self.try_mark_merge_blocked().is_err() {
            self.force_status_with_reason(StageStatus::MergeBlocked, &sentence);
        }
    }

    /// Record what the merge did with the operator's stashed changes. Never
    /// cleared: it stays as the stage's merge note. A restored note does not
    /// replace an unrestored one, whose stash still holds work the checkout
    /// lacks; a new unrestored note replaces any. Every unrestored backup ref
    /// is also kept in `unrestored`, so a newer note never hides an older one.
    pub fn record_merge_stash(&mut self, stash: StashReapply) {
        if !stash.restored && !self.merge.unrestored.contains(&stash.backup_ref) {
            self.merge.unrestored.push(stash.backup_ref.clone());
        }
        let keeps_unrestored = stash.restored
            && self
                .merge
                .stash
                .as_ref()
                .is_some_and(|existing| !existing.restored);
        if !keeps_unrestored {
            self.merge.stash = Some(stash);
        }
    }

    /// Mark the stage as having merge conflicts.
    ///
    /// This sets both the status to MergeConflict and the merge_conflict flag.
    /// The stage work is complete but cannot be merged due to conflicts.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_merge_conflict(&mut self) -> anyhow::Result<()> {
        self.try_transition(StageStatus::MergeConflict)?;
        self.merge_conflict = true;
        Ok(())
    }

    /// Complete the merge: clear `merge_conflict` and set `merged`. Unless the stage is
    /// already `Completed` (auto-merge disabled leaves it there, and `loom stage merge`
    /// then runs against it), also transition to `Completed` and stamp the timestamps.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_complete_merge(&mut self) -> anyhow::Result<()> {
        if self.status != StageStatus::Completed {
            self.try_transition(StageStatus::Completed)?;
            self.stamp_completed();
        }
        self.merge_conflict = false;
        self.merged = true;
        Ok(())
    }

    /// Mark the stage as merge blocked (merge failed with actual error, not conflicts).
    ///
    /// This indicates the merge operation failed due to an error (not conflicts).
    /// The stage can be retried by transitioning back to Executing.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_merge_blocked(&mut self) -> anyhow::Result<()> {
        self.try_transition(StageStatus::MergeBlocked)
    }

    /// Why a stage whose branch has zero commits beyond `target` goes to human
    /// review instead of merging: a no-op merge would stand for work that was
    /// never committed.
    pub fn zero_commit_reason(stage_id: &str, target: &str) -> String {
        let branch = crate::git::branch::branch_name_for_stage(stage_id);
        format!(
            "branch {branch} has zero commits beyond {target}: the agent never committed work \
             for this stage. Re-queue it with `loom stage human-review {stage_id} --approve`, or \
             redo it manually."
        )
    }

    /// Move the stage to `NeedsHumanReview` with `reason`, whatever its
    /// status; the transition drops any merge block. The merge gate uses it
    /// for a branch that touches a control path.
    pub fn route_to_review(&mut self, reason: &str) {
        self.force_status_with_reason(StageStatus::NeedsHumanReview, reason);
        self.replace_review_reason(reason.to_string());
    }

    /// Forget the typed merge block. Every status change out of `MergeBlocked`
    /// drops it too (`retire_issues_of`, labelled by the transition), so a
    /// stale reason never outlives the status it explained. The
    /// `failure_info` that `block_merge` wrote goes with the block, into
    /// `issue_history`; with no block recorded, an unrelated `failure_info`
    /// stays.
    pub fn clear_merge_block(&mut self) {
        if self.merge.block.take().is_some() {
            self.retire_failure("merge block cleared");
        }
    }

    /// Hand a stage whose resolver committed the merge back to a stage
    /// session: `Queued`, with `refusal` kept for the signal and the fix and
    /// stall budgets fresh. Changes nothing unless the stage is in
    /// `MergeConflict` or `MergeBlocked`.
    ///
    /// A conflict steps through `Blocked` because the table keeps
    /// `MergeConflict -> Queued` illegal, so no retry, resume or recovery path
    /// can requeue an unresolved conflict. The callers have proved a
    /// merge commit on the branch (`has_merge_commit_since`).
    pub fn try_hand_back_merge(&mut self, refusal: String) -> anyhow::Result<()> {
        match self.status {
            StageStatus::MergeConflict => {
                self.try_transition(StageStatus::Blocked)?;
                self.try_transition(StageStatus::Queued)?;
            }
            StageStatus::MergeBlocked => self.try_transition(StageStatus::Queued)?,
            // Not a merge state: `Queued` may be legal from here (human
            // review), but a handback is not.
            ref status => anyhow::bail!(
                "Invalid stage status transition: {status} -> Queued (not MergeConflict or \
                 MergeBlocked)"
            ),
        }
        self.merge_conflict = false;
        self.release_session();
        self.merge.handbacks += 1;
        self.merge.handback_refusal = Some(refusal);
        self.fix_attempts = 0;
        self.stall_recoveries = 0;
        Ok(())
    }

    /// Move the stage to `MergeConflict`, clearing any merge block.
    pub fn enter_merge_conflict(&mut self) {
        if self.status == StageStatus::MergeConflict {
            self.clear_merge_block();
            self.merge_conflict = true;
        } else if let Err(error) = self.try_mark_merge_conflict() {
            self.force_status_with_reason(
                StageStatus::MergeConflict,
                &format!("the merge conflicts but the transition was illegal: {error}"),
            );
            self.merge_conflict = true;
        }
    }
}

#[cfg(test)]
mod tests;
