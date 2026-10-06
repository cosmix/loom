//! `loom stage merge <own> --resolved`, relayed from a Merge session.
//!
//! The resolver merged the target into the stage branch in the stage
//! worktree (`git merge --no-commit`, then a relayed `loom stage commit` the
//! daemon applies). The daemon checks that worktree through pinned git
//! (`check_resolved_worktree`: no merge in progress, no unmerged path, no
//! tracked change, the stage's recorded commit still in the branch), then lands the merge
//! with `merge_stage` through the merge gate. `merged = true` is written only
//! after ancestry proves the stage's commit is in the target. The worktree is
//! not removed here: the resolver is still running in it, and its exit
//! (`handle_merge_session_completed`) cleans up.

use crate::git::merge::check_resolved_worktree;
use crate::models::session::Session;
use crate::models::stage::StageStatus;

use super::super::merge_handler::Landing;
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
        let target = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        if let Err(reason) = check_resolved_worktree(
            &self.config.repo_root,
            stage_id,
            stage.completed_commit.as_deref(),
        ) {
            return Settle::Refused(reason);
        }
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
