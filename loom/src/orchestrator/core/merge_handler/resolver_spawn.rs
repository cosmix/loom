//! One pass of the daemon's merge-resolver spawn loop over a single
//! `MergeConflict`/`MergeBlocked` stage. Such a stage never counts as
//! terminal for the watch-mode exit (`recovery.rs::stage_file_is_terminal`),
//! so each pass leaves a resolver running, retries a transient failure, or
//! routes the stage to `NeedsHumanReview`. The stage copy a pass works from
//! can go stale while it runs git, so every routing is guarded
//! (`route_merge_stage_to_review`) and the stage is re-read before a resolver
//! is reserved. Relocated out of `merge_handler.rs` to keep it within its
//! maintainability-ledger line budget.

use anyhow::{Context, Result};

use crate::git::branch::{branch_name_for_stage, resolve_target_branch};
use crate::git::cleanup::branch_exists_strict;
use crate::git::merge::{merge_tree, TreeMerge};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};
use crate::orchestrator::signals::{find_live_merge_session_for_stage, generate_merge_signal};
use crate::orchestrator::terminal::backend::merge_resolver_worktree;

use super::landing::Landing;
use super::resolver_attempts::{
    merge_resolver_attempts, ReservedAttempt, MAX_MERGE_RESOLVER_ATTEMPTS,
};
use super::review_route::awaits_merge;

impl Orchestrator {
    /// Spawn a merge resolver for `stage` when one is due; returns whether one
    /// spawned.
    ///
    /// Nothing happens while the tick's target guard check left the target
    /// held: the probes below read the live target and can route the stage to
    /// review, and a resolver merges the target into the stage branch, which
    /// would carry the held commits past a later restore.
    ///
    /// The merge gate holds a control-path branch for human review, stopping
    /// any resolver running on it, and a live resolver (tracked, or found
    /// through its signal) is left to finish. Otherwise a missing stage or
    /// target branch, a spent resolver budget, or an attempt the counter
    /// cannot record routes the stage to human review, and a stage that left
    /// its merge state meanwhile gets no resolver. A merge that has become
    /// clean (the target moved, say) is landed here, with no resolver. The
    /// resolver then spawns
    /// with its attempt recorded first; a failed spawn gives the attempt back
    /// and is retried next tick, unless `report_merge_spawn_failure` routes it
    /// to review.
    pub(super) fn spawn_resolver_if_due(&mut self, stage: &Stage) -> bool {
        if self.target_held() {
            return false;
        }
        let stage_id = stage.id.as_str();
        let branch = branch_name_for_stage(stage_id);
        let target_branch = resolve_target_branch(&self.config.base_branch, &self.config.repo_root);
        if self.gate_holds_merge_stage(stage_id, &branch, &target_branch)
            || self.cleanup_stale_merge_session(stage_id)
            || self.signalled_resolver_blocks_spawn(stage_id)
            || self.missing_branch_blocks_spawn(stage_id, &branch, &target_branch)
            || self.missing_worktree_blocks_spawn(stage_id)
        {
            return false;
        }
        if self.clean_merge_settled(stage_id, &branch, &target_branch) {
            return false;
        }
        let attempts = merge_resolver_attempts(&self.config.work_dir, stage_id);
        if attempts >= MAX_MERGE_RESOLVER_ATTEMPTS {
            self.escalate_merge_resolver_exhausted(stage_id, attempts);
            return false;
        }
        if !self.stage_still_awaits_merge(stage_id) {
            return false;
        }
        let Some(attempt) = self.reserve_resolver_attempt(stage_id, attempts) else {
            return false;
        };
        match self.spawn_merge_resolution_session(stage, attempt) {
            Ok(()) => true,
            Err(error) => {
                self.report_merge_spawn_failure(stage_id, error);
                false
            }
        }
    }

    /// Returns true when the merge of `branch` into `target` is clean now and
    /// `stage_id` was settled here instead of by a resolver: landed and
    /// cleaned up, or already recorded as held, conflicting or blocked by the
    /// landing, or — when the landing left no proof the stage's commit is in
    /// the target — routed to human review. A merge that conflicts, cannot be
    /// computed, or whose landing failed falls through to a resolver, whose
    /// signal carries the reason.
    fn clean_merge_settled(&mut self, stage_id: &str, branch: &str, target: &str) -> bool {
        let tree = merge_tree(
            &self.config.repo_root,
            &format!("refs/heads/{target}"),
            &format!("refs/heads/{branch}"),
        );
        match tree {
            Ok(TreeMerge::Clean { .. }) => {}
            Ok(TreeMerge::Conflict { .. }) => return false,
            Err(error) => {
                tracing::warn!(stage_id = %stage_id, error = %format!("{error:#}"), "Could not check whether the merge is clean; spawning a resolver");
                return false;
            }
        }
        if self.clean_merge_lacks_original_work(stage_id, branch) {
            return true;
        }
        match self.land_stage_merge(stage_id, target) {
            Landing::Merged => self.cleanup_resolved_merge(stage_id, target),
            Landing::Held | Landing::Conflict(_) | Landing::Blocked(_) => {}
            Landing::Unproven => {
                let reason = format!(
                    "the merge landed in {target} but the stage's recorded commit is not in it; \
                     its work may be lost: inspect {branch}"
                );
                self.route_merge_stage_to_review(stage_id, reason, None);
            }
            Landing::Failed(error) => {
                tracing::warn!(stage_id = %stage_id, %error, "Landing a clean merge failed; spawning a resolver");
                return false;
            }
        }
        true
    }

    fn clean_merge_lacks_original_work(&mut self, stage_id: &str, branch: &str) -> bool {
        let Ok(stage) = self.load_stage(stage_id) else {
            return false;
        };
        let survived = stage.completed_commit.as_deref().is_some_and(|commit| {
            crate::git::branch::is_ancestor_of(commit, branch, &self.config.repo_root)
                .unwrap_or(false)
        });
        if !survived {
            let reason = format!(
                "the stage's recorded commit is not in {branch}, or ancestry cannot be verified; \
                 no merge was attempted: inspect {branch}"
            );
            self.route_merge_stage_to_review(stage_id, reason, None);
        }
        !survived
    }

    /// Returns true — after stopping any resolver running for `stage_id` and
    /// routing the stage to `NeedsHumanReview` — when the merge gate holds
    /// `branch`.
    fn gate_holds_merge_stage(&mut self, stage_id: &str, branch: &str, target: &str) -> bool {
        let Some(mut reason) = self.merge_gate_reason(stage_id, branch, target) else {
            return false;
        };
        if let Some(stopped) = self.stop_gated_resolvers(stage_id) {
            clear_status_line();
            eprintln!("Stage '{stage_id}': branch {branch} touches a control path; {stopped}");
            reason = format!("{reason}. {stopped}");
        }
        self.route_merge_stage_to_review(stage_id, reason, None);
        true
    }

    /// Returns true when a merge signal names a live resolver for `stage_id`,
    /// or — after routing the stage to `NeedsHumanReview` — when a merge
    /// signal that cannot be read cannot be attributed to a stage either. It
    /// is then unknown whether a resolver is running: spawning could put a
    /// second one in the stage worktree, and waiting would never end, since
    /// only the daemon writes merge signals while it runs and nothing repairs
    /// a broken one.
    fn signalled_resolver_blocks_spawn(&mut self, stage_id: &str) -> bool {
        let error = match find_live_merge_session_for_stage(stage_id, &self.config.work_dir) {
            Ok(live) => return live.is_some(),
            Err(error) => error,
        };
        let steps = self.manual_merge_steps(stage_id);
        let reason = format!(
            "merge resolver not spawned; {steps}. Could not tell whether one is already \
             running: {error:#}"
        );
        self.route_merge_stage_to_review(stage_id, reason, None);
        true
    }

    /// Returns true — after routing `stage_id` to `NeedsHumanReview` — when
    /// the stage branch `branch` or the target branch `target` does not
    /// exist: no resolver can merge it, and no retry brings it back. A missing
    /// stage branch is never taken as merged; that is the phantom-merge risk
    /// `MergeState::BranchMissing` guards against. A check that fails falls
    /// through to the spawn, whose git failures are retried each tick.
    fn missing_branch_blocks_spawn(&mut self, stage_id: &str, branch: &str, target: &str) -> bool {
        for (role, name) in [("branch", branch), ("target branch", target)] {
            match branch_exists_strict(name, &self.config.repo_root) {
                Ok(true) => {}
                Ok(false) => {
                    let steps = self.manual_merge_steps(stage_id);
                    let reason = format!("{role} {name} is missing; restore it, then {steps}");
                    self.route_merge_stage_to_review(stage_id, reason, None);
                    return true;
                }
                Err(error) => tracing::warn!(
                    stage_id = %stage_id,
                    branch = %name,
                    %error,
                    "Failed to check that a merge branch exists; attempting the resolver spawn"
                ),
            }
        }
        false
    }

    /// Returns true — after routing `stage_id` to `NeedsHumanReview` — when
    /// the stage worktree `.worktrees/<id>` is missing: the resolver works
    /// there, so none can spawn until it is recreated or the merge is done by
    /// hand.
    fn missing_worktree_blocks_spawn(&mut self, stage_id: &str) -> bool {
        if merge_resolver_worktree(&self.config.repo_root, stage_id).is_ok() {
            return false;
        }
        let steps = self.manual_merge_steps(stage_id);
        let reason = format!(
            "the stage worktree .worktrees/{stage_id} is missing and the merge resolver works \
             there; recreate it or merge by hand, then {steps}"
        );
        self.route_merge_stage_to_review(stage_id, reason, None);
        true
    }

    /// Whether the fresh on-disk `stage_id` is still `MergeConflict` or
    /// `MergeBlocked`. A stage merged while the pass ran (by `loom stage
    /// merge`, say) must get no resolver; one that cannot be re-read is
    /// retried next tick.
    fn stage_still_awaits_merge(&self, stage_id: &str) -> bool {
        match self.load_stage(stage_id) {
            Ok(stage) if awaits_merge(&stage.status) => true,
            Ok(stage) => {
                tracing::info!(
                    stage_id = %stage_id,
                    status = %stage.status,
                    "Stage left its merge state meanwhile; spawning no merge resolver"
                );
                false
            }
            Err(error) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    error = %format!("{error:#}"),
                    "Failed to re-read stage before reserving a merge resolver; retrying next tick"
                );
                false
            }
        }
    }

    /// Spawn a merge resolution session for a stage with merge issues.
    /// `attempt` is kept once the backend spawns the session; any failure
    /// before that gives it back, and `launch_resolver` settles a backend
    /// failure. A spawned resolver stays tracked when its record cannot be
    /// saved: its signal then reads as stale, and only tracking holds the stage.
    fn spawn_merge_resolution_session(
        &mut self,
        stage: &Stage,
        attempt: ReservedAttempt,
    ) -> Result<()> {
        let source_branch = branch_name_for_stage(&stage.id);

        let target_branch = resolve_target_branch(&self.config.base_branch, &self.config.repo_root);

        // Conflicting files from a merge-tree dry run, which touches no working tree.
        let conflicting_files =
            match merge_tree(&self.config.repo_root, &target_branch, &source_branch)? {
                TreeMerge::Clean { .. } => Vec::new(),
                TreeMerge::Conflict { paths, .. } => paths,
            };
        let session = Session::new_merge(source_branch.clone(), target_branch.clone());

        let signal_path = generate_merge_signal(
            &session,
            stage,
            &source_branch,
            &target_branch,
            &conflicting_files,
            &self.config.work_dir,
        )
        .context("Failed to generate merge signal")?;

        let spawned_session = self.launch_resolver(stage, session, &signal_path, attempt)?;

        announce_resolver(&stage.id, &spawned_session.id, &conflicting_files);
        self.active_sessions
            .insert(stage.id.clone(), spawned_session.clone());
        if let Err(error) = self.save_session(&spawned_session) {
            eprintln!("Warning: Failed to save the merge resolver's session record: {error:#}");
        }
        Ok(())
    }

    /// Record the attempt of the resolver about to spawn for `stage_id`, whose
    /// counter read `attempts`. When the counter cannot be written, no resolver
    /// spawns and the stage goes to `NeedsHumanReview`: an uncounted resolver
    /// could respawn without bound.
    fn reserve_resolver_attempt(
        &mut self,
        stage_id: &str,
        attempts: u32,
    ) -> Option<ReservedAttempt> {
        let error = match ReservedAttempt::record(&self.config.work_dir, stage_id, attempts) {
            Ok(attempt) => return Some(attempt),
            Err(error) => error,
        };
        let steps = self.manual_merge_steps(stage_id);
        let reason = format!(
            "merge resolver not spawned; {steps}. Its attempt could not be recorded: {error}"
        );
        self.route_merge_stage_to_review(stage_id, reason, None);
        None
    }
}

/// Tell the console which resolver spawned for `stage_id` and what it merges.
fn announce_resolver(stage_id: &str, session_id: &str, conflicting_files: &[String]) {
    clear_status_line();
    eprintln!("Spawned merge resolution session for stage '{stage_id}': {session_id}");
    if !conflicting_files.is_empty() {
        eprintln!("  Conflicting files:");
        for file in conflicting_files {
            eprintln!("    - {file}");
        }
    }
}

#[cfg(test)]
#[path = "spawn_test_fixtures.rs"]
pub(super) mod test_fixtures;

#[cfg(test)]
#[path = "spawn_loop_tests.rs"]
mod tests;
