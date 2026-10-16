//! Merge session handling and auto-merge logic

use anyhow::Result;
use chrono::Utc;

use crate::git::cleanup::CleanupConfig;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::StageStatus;
use crate::orchestrator::merge_lifecycle::{self, CleanupOutcome, MergeLifecycle};

use super::persistence::Persistence;
use super::{clear_status_line, Orchestrator};

mod auto_merge_dispatch;
mod auto_merge_outcome;
mod blocked_retry;
mod landing;
mod leftover_sweep;
mod merge_gate;
pub(super) mod resolver_attempts;
mod resolver_exit;
mod resolver_give_up;
mod resolver_spawn;
mod resolver_stop;
mod review_route;
mod spawn_failure;

pub(super) use landing::Landing;
use leftover_sweep::report_deferred_cleanup;
use resolver_attempts::attempts_file;
use review_route::awaits_merge;

impl Orchestrator {
    /// Verify merge succeeded and update stage state accordingly.
    ///
    /// This helper encapsulates the common pattern of verifying a merge via git ancestry
    /// against the target tip the target guard accepted, and updating stage state.
    ///
    /// A failed verification always moves the stage to `MergeBlocked` with the
    /// reason in `failure_info`, whatever its prior status.
    ///
    /// Returns `true` if the merge was verified successful via git ancestry.
    /// Returns `false` otherwise. Caller must NOT assume `merged=true` on false.
    fn verify_and_finalize_merge(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        stage_id: &str,
        target_branch: &str,
    ) -> bool {
        let completed_commit = match stage.completed_commit.clone() {
            Some(commit) => commit,
            None => {
                self.persist_merge_blocked(
                    stage,
                    stage_id,
                    "cannot verify merge: no completed_commit recorded; rerun completion",
                );
                return false;
            }
        };

        match self.merged_into_accepted(&completed_commit, target_branch) {
            Ok(true) => self.persist_verified_merge(stage, stage_id, &completed_commit),
            Ok(false) => {
                self.persist_merge_blocked(
                    stage,
                    stage_id,
                    &format!(
                        "merge verification failed: commit {completed_commit} is not in \
                         {target_branch} as loom accepted it"
                    ),
                );
                false
            }
            Err(error) => {
                self.persist_merge_blocked(
                    stage,
                    stage_id,
                    &format!("merge verification errored: {error:#}"),
                );
                false
            }
        }
    }

    fn persist_verified_merge(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        stage_id: &str,
        completed_commit: &str,
    ) -> bool {
        let updated = self.update_stage(stage_id, |current| {
            anyhow::ensure!(
                current.completed_commit.as_deref() == Some(completed_commit),
                "completed_commit changed during merge verification"
            );
            current.merged = true;
            current.clear_merge_block();
            Ok(())
        });
        match updated {
            Ok(updated) => {
                *stage = updated;
                true
            }
            Err(error) => {
                tracing::warn!(stage_id = %stage_id, %error, "Failed to persist verified merge");
                false
            }
        }
    }

    /// Persist a failed auto-merge as `MergeBlocked`, recording `error` in
    /// `failure_info`. Applies to `Completed` stages too, so `loom status`
    /// shows the failure and `loom stage merge` can retry it.
    fn persist_merge_blocked(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        stage_id: &str,
        error: &str,
    ) {
        let first_line = error.lines().next().unwrap_or(error);
        let updated = self.update_stage(stage_id, |current| {
            current.clear_merge_block();
            current.failure_info = Some(FailureInfo {
                failure_type: FailureType::InfrastructureError,
                detected_at: Utc::now(),
                evidence: error.lines().map(str::to_owned).collect(),
            });
            if current.try_mark_merge_blocked().is_err() {
                current.force_status_with_reason(
                    StageStatus::MergeBlocked,
                    &format!("auto-merge failed: {first_line}"),
                );
            }
            Ok(())
        });
        match updated {
            Ok(updated) => *stage = updated,
            Err(error) => {
                tracing::warn!(stage_id = %stage_id, %error, "Failed to persist MergeBlocked");
                return;
            }
        }
        if stage.status == StageStatus::MergeBlocked {
            let _ = self.graph.mark_status(stage_id, StageStatus::MergeBlocked);
            clear_status_line();
            eprintln!("Stage '{stage_id}' auto-merge failed: {first_line}");
            eprintln!("{}", self.merge_blocked_hint(stage_id));
        }
    }

    /// Run the deferred worktree/branch cleanup for a stage already marked
    /// merged (e.g. by `loom stage complete`).
    ///
    /// `loom stage complete` intentionally skips `cleanup_after_merge` when it
    /// runs from inside the worktree it would delete — removing the live
    /// agent session's cwd breaks every remaining Claude Code hook spawn for
    /// that session (Stop, SessionEnd, the trailing PostToolUse all fail with
    /// `posix_spawn '/bin/sh'` ENOENT once their cwd is gone). A session still
    /// running for the stage defers the cleanup, which the leftover sweep
    /// retries each tick. The outcome is reported (not just logged at `warn`)
    /// so a failed or refused cleanup is visible on the daemon's console
    /// instead of only in tracing output nobody watches.
    fn cleanup_already_merged(&self, stage_id: &str) -> CleanupOutcome {
        let target_branch = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        let outcome = MergeLifecycle::new(stage_id, &self.config.repo_root, &self.config.work_dir)
            .cleanup(&target_branch, &CleanupConfig::quiet());
        report_deferred_cleanup(stage_id, &outcome);
        outcome
    }

    /// Shared tail for `try_auto_merge`'s three merge-succeeded outcomes
    /// (Success, FastForward, AlreadyUpToDate): re-verify ancestry via
    /// `verify_and_finalize_merge`, and only on success run the primitive's
    /// post-merge base-reconcile + cleanup and print the summary line.
    fn finalize_auto_merge(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        stage_id: &str,
        target_branch: &str,
        summary: &str,
    ) -> bool {
        let success = self.verify_and_finalize_merge(stage, stage_id, target_branch);
        if success {
            merge_lifecycle::finish_verified_merge(
                stage_id,
                &self.config.repo_root,
                &self.config.work_dir,
                target_branch,
                &CleanupConfig::quiet(),
            );
            clear_status_line();
            eprintln!("Stage '{stage_id}' {summary}");
        }
        success
    }

    /// Spawn merge resolution sessions for stages in MergeConflict or MergeBlocked status.
    ///
    /// Called during the main loop to detect stages that need merge resolution;
    /// `spawn_resolver_if_due` decides each one.
    pub fn spawn_merge_resolution_sessions(&mut self) -> Result<usize> {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return Ok(0);
        }
        let mut spawned = 0;
        self.prune_retry_memos();
        for entry in std::fs::read_dir(&stages_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Extract stage ID from filename using the canonical parser
            let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let stage_id = match crate::fs::stage_files::extract_stage_id(filename) {
                Some(id) => id,
                None => continue,
            };

            // Load stage and check status
            let stage = match self.load_stage(&stage_id) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Only handle MergeConflict and MergeBlocked statuses
            if !awaits_merge(&stage.status) {
                continue;
            }

            // A typed merge block needs no resolver: retry the merge itself.
            if stage.status == StageStatus::MergeBlocked && stage.merge.block.is_some() {
                self.retry_blocked_merge(&stage);
            } else if self.spawn_resolver_if_due(&stage) {
                spawned += 1;
            }
        }

        Ok(spawned)
    }

    /// Clear the persisted merge-resolver attempt counter for `stage_id`.
    ///
    /// Called once a merge is finalized so a later, unrelated conflict on the
    /// same stage id starts with a fresh budget.
    fn clear_merge_resolver_attempts(&self, stage_id: &str) {
        let path = attempts_file(&self.config.work_dir, stage_id);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(
                    stage_id = %stage_id,
                    error = %e,
                    "Failed to clear merge-resolver attempt counter"
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "merge_handler_attempt_tests.rs"]
mod merge_handler_attempt_tests;

#[cfg(test)]
mod tests;
