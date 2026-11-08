//! `loom stage merge <own> --resolved`, relayed from a Merge session.
//!
//! The resolver merged the target into the stage branch in the stage
//! worktree (`git merge --no-commit`, then a relayed `loom stage commit` the
//! daemon applies). The daemon checks that worktree through pinned git
//! (`check_resolved_worktree`: no merge in progress, no unmerged path, no
//! tracked change, the stage's recorded commit still in the branch), then lands the merge
//! with `merge_stage` through the merge gate. `merged = true` is written only
//! after ancestry proves the stage's commit is in the target. Content that
//! fails completion verification hands the stage back to a stage session
//! (`hand_merge_back`). The worktree is not removed here: the resolver is
//! still running in it, and its exit (`handle_merge_session_completed`)
//! cleans up. A check the resolver ran is no input to any of this; one still
//! running is stopped once the worktree is accepted, before the gates run.

use crate::git::merge::check_resolved_worktree;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::busy::{self, BusyOp};
use crate::orchestrator::tick::Phase;

use super::super::merge_handler::{trace_verification_output, Landing};
use super::super::persistence::Persistence;
use super::super::Orchestrator;
use super::Settle;

impl Orchestrator {
    /// Land the merge `session` resolved for `stage_id`.
    pub(super) fn resolve_merge_from_inbox(
        &mut self,
        _session: &Session,
        stage_id: &str,
    ) -> Settle {
        let stage = match self.load_stage(stage_id) {
            Ok(stage) => stage,
            Err(error) => return Settle::Refused(format!("{error:#}")),
        };
        if !matches!(
            stage.status,
            StageStatus::MergeConflict | StageStatus::MergeBlocked
        ) {
            return Settle::Refused(format!(
                "stage '{stage_id}' is {}, not MergeConflict or MergeBlocked",
                stage.status
            ));
        }
        if let Err(reason) = check_resolved_worktree(
            &self.config.repo_root,
            stage_id,
            stage.completed_commit.as_deref(),
        ) {
            return Settle::Refused(reason);
        }
        if let Err(reason) = self.stop_running_check(stage_id) {
            return Settle::Refused(reason);
        }
        let verified = busy::while_busy(
            &self.config.work_dir,
            Phase::Sync,
            BusyOp::MergeVerification,
            stage_id,
            || {
                crate::verify::completion::resolved::verify_resolved_completion(
                    &stage,
                    &self.config.work_dir,
                    trace_verification_output,
                )
            },
        );
        if let Err(error) = verified {
            return self.settle_failed_verification(&stage, stage_id, &error);
        }
        self.land_verified_merge(stage_id)
    }

    /// Land the merge whose content passed verification, and settle how it ended.
    fn land_verified_merge(&mut self, stage_id: &str) -> Settle {
        let target = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        match self.land_stage_merge(stage_id, &target) {
            // The landing recorded why it held the merge; say that.
            Landing::Held => {
                let review = self
                    .load_stage(stage_id)
                    .ok()
                    .and_then(|held| held.review_reason);
                Settle::Refused(held_reason(review.as_deref()))
            }
            landing => settle_for_landing(landing, stage_id, &target),
        }
    }

    /// Settle `--resolved` for content that failed completion verification
    /// with `error`. A typed block is refused: loom retries such a merge every
    /// tick, and a handback would cancel the retry of a resolution it already
    /// accepted. A branch with no merge commit since the stage's completed
    /// commit is refused, since a clean tree proves no merge happened. Only a
    /// proven committed merge is handed back to a stage session.
    fn settle_failed_verification(
        &mut self,
        stage: &Stage,
        stage_id: &str,
        error: &anyhow::Error,
    ) -> Settle {
        let refusal = format!("resolved content failed completion verification: {error:#}");
        if stage.merge.block.is_some() {
            return Settle::Refused(refusal);
        }
        if !self.resolver_merge_committed(stage) {
            return Settle::Refused(format!(
                "{refusal}; the branch has no merge commit since the stage's completed commit: \
                 merge the target with git merge --no-commit --no-ff, commit it with loom stage \
                 commit, then rerun --resolved"
            ));
        }
        match self.hand_merge_back(stage_id, &refusal) {
            Ok(note) => Settle::Applied(Some(note)),
            Err(why) => Settle::Refused(why),
        }
    }
}

/// The reply for how landing the resolved merge ended.
pub(super) fn settle_for_landing(landing: Landing, stage_id: &str, target: &str) -> Settle {
    match landing {
        Landing::Merged => Settle::Applied(Some(format!(
            "merged into '{target}'; the worktree is removed after this session exits"
        ))),
        Landing::Held => Settle::Refused(held_reason(None)),
        // A resolver never commits through git: `git merge --continue` and a
        // bare `git merge` commit inside the sandbox, which cannot sign.
        Landing::Conflict(paths) => Settle::Refused(format!(
            "'{target}' moved and conflicts again in {}: merge it into this worktree again \
             with git merge --no-commit --no-ff {target}, resolve, stage the resolution, \
             commit it with loom stage commit and wait for it with loom request status <id> \
             --wait 90, then rerun --resolved; never run git merge --continue",
            paths.join(", ")
        )),
        Landing::Blocked(block) => Settle::Applied(Some(format!(
            "resolution accepted; the merge is blocked: {block}. Loom retries it every tick"
        ))),
        Landing::Unproven => Settle::Refused(format!(
            "no ancestry proof that stage '{stage_id}' landed in '{target}'; merged stays false"
        )),
        Landing::Failed(error) => Settle::Refused(error),
    }
}

/// Why a held landing went to human review: the stage's own review reason
/// when it could be read, else both causes a hold has.
fn held_reason(review_reason: Option<&str>) -> String {
    match review_reason {
        Some(reason) => format!("routed to human review: {reason}"),
        None => format!(
            "routed to human review: the stage branch touches a control path ({}), or \
             loom's merge commit could not be signed",
            Orchestrator::CONTROL_PATHS
        ),
    }
}
