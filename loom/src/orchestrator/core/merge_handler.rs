//! Merge session handling and auto-merge logic

use anyhow::Result;
use chrono::Utc;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::CleanupConfig;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::StageStatus;
use crate::orchestrator::auto_merge::{attempt_auto_merge, is_auto_merge_enabled};
use crate::orchestrator::merge_lifecycle::{self, CleanupOutcome, MergeLifecycle};
use crate::verify::transitions::load_stage;

use super::persistence::Persistence;
use super::{clear_status_line, Orchestrator};

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
        let branch_name = branch_name_for_stage(stage_id);
        let completed_commit = match stage
            .completed_commit
            .clone()
            .or_else(|| crate::git::get_branch_head(&branch_name, &self.config.repo_root).ok())
        {
            Some(commit) => commit,
            None => {
                tracing::error!(
                    stage_id = %stage_id,
                    branch = %branch_name,
                    "Cannot verify merge without completed_commit or branch HEAD"
                );
                self.persist_merge_blocked(
                    stage,
                    stage_id,
                    &format!(
                        "cannot verify merge: no completed_commit recorded and branch \
                         {branch_name} is missing"
                    ),
                );
                return false;
            }
        };

        match self.merged_into_accepted(&completed_commit, target_branch) {
            Ok(true) => self.persist_verified_merge(stage, stage_id, &completed_commit),
            Ok(false) => {
                tracing::error!(
                    stage_id = %stage_id,
                    commit = %completed_commit,
                    target = %target_branch,
                    "Merge verification failed: commit not in target branch"
                );
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
                tracing::error!(
                    stage_id = %stage_id,
                    %error,
                    "Merge verification errored"
                );
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
            match current.completed_commit.as_deref() {
                Some(fresh) if fresh != completed_commit => {
                    anyhow::bail!("completed_commit changed during merge verification")
                }
                None => current.completed_commit = Some(completed_commit.to_string()),
                Some(_) => {}
            }
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

    /// Attempt auto-merge for a completed stage.
    ///
    /// Returns `true` if the merge succeeded or was not needed (stage can be marked Completed).
    /// Returns `false` if the merge did not land: the stage is `MergeConflict` (the spawn
    /// loop gives it a resolver), `MergeBlocked` (retried every tick) or in human review.
    pub(super) fn try_auto_merge(&mut self, stage_id: &str) -> bool {
        let mut stage = match load_stage(stage_id, &self.config.work_dir) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: Failed to load stage for auto-merge check: {e}");
                // If we can't load the stage, allow completion to proceed
                return true;
            }
        };

        // An already merged stage (e.g. by `loom stage complete`) is not merged
        // again: a redundant merge after a partial cleanup would overwrite the
        // terminal Completed status with MergeConflict/MergeBlocked.
        if stage.merged {
            self.cleanup_already_merged(stage_id);
            return true;
        }

        // O-20: `read_plan_level_auto_merge` warns about an unreadable plan.
        let plan_auto_merge = self.read_plan_level_auto_merge(stage_id);
        if !is_auto_merge_enabled(&stage, self.config.auto_merge, plan_auto_merge) {
            // Leave the stage Completed + !merged for `loom stage merge`; merged=true
            // here would satisfy dependents without a merge.
            tracing::info!(stage_id = %stage_id, "auto-merge disabled; run `loom stage merge` to merge manually");
            return true;
        }

        let target_branch = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        if self.auto_merge_precheck_blocks(stage_id, &target_branch) {
            return false;
        }
        self.capture_completed_commit(&mut stage, stage_id);

        clear_status_line();
        eprintln!("Auto-merging stage '{stage_id}'...");
        MergeLifecycle::new(stage_id, &self.config.repo_root, &self.config.work_dir)
            .reconcile_overlay();
        let outcome = attempt_auto_merge(
            &stage,
            &self.config.repo_root,
            &self.config.work_dir,
            &target_branch,
        );
        self.apply_auto_merge_outcome(&mut stage, stage_id, &target_branch, outcome)
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

    /// Read the plan-level `auto_merge` flag from the active plan file.
    ///
    /// Returns:
    /// - `Some(flag)` when the plan declares an explicit `auto_merge` value.
    /// - `None` when there is genuinely no plan-level setting (no config, no
    ///   `source_path`, or the plan omits `auto_merge`) — callers fall back to
    ///   the daemon/stage default.
    ///
    /// O-20: when the plan path is known (a `source_path` exists) but the file
    /// cannot be read or its metadata cannot be parsed, this logs a warning and
    /// returns `None`. A plan-level `auto_merge: false` could be hiding in an
    /// unreadable plan; defaulting to enabled without surfacing that would
    /// silently override the user's intent.
    fn read_plan_level_auto_merge(&self, stage_id: &str) -> Option<bool> {
        let config = match crate::fs::load_config(&self.config.work_dir) {
            Ok(Some(c)) => c,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    error = %e,
                    "Failed to load config for plan-level auto_merge; \
                     falling back to default auto-merge setting"
                );
                return None;
            }
        };

        // No source_path → no plan file to consult; legitimate None.
        let source_path = config.source_path()?;
        let plan_path = self.config.repo_root.join(&source_path);

        let plan_content = match std::fs::read_to_string(&plan_path) {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    plan_path = %plan_path.display(),
                    error = %e,
                    "Plan file is referenced but could not be read; a plan-level \
                     auto_merge:false may be ignored — falling back to default auto-merge setting"
                );
                return None;
            }
        };

        let yaml_content = match crate::plan::parser::extract_yaml_metadata(&plan_content) {
            Ok(y) => y,
            Err(e) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    plan_path = %plan_path.display(),
                    error = %e,
                    "Failed to extract YAML metadata from plan; a plan-level \
                     auto_merge:false may be ignored — falling back to default auto-merge setting"
                );
                return None;
            }
        };

        match crate::plan::parser::parse_and_validate(&yaml_content) {
            Ok(metadata) => metadata.loom.auto_merge,
            Err(e) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    plan_path = %plan_path.display(),
                    error = %e,
                    "Failed to parse plan metadata; a plan-level auto_merge:false may \
                     be ignored — falling back to default auto-merge setting"
                );
                None
            }
        }
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
mod tests {
    #[test]
    fn test_merge_blocked_to_completed_transition_is_valid() {
        use crate::models::stage::StageStatus;

        let status = StageStatus::MergeBlocked;
        assert!(
            status.can_transition_to(&StageStatus::Completed),
            "MergeBlocked -> Completed should be a valid transition"
        );

        let result = status.try_transition(StageStatus::Completed);
        assert!(
            result.is_ok(),
            "MergeBlocked -> Completed transition should succeed"
        );
    }

    #[test]
    fn test_merge_conflict_to_completed_transition_is_valid() {
        use crate::models::stage::StageStatus;

        let status = StageStatus::MergeConflict;
        assert!(
            status.can_transition_to(&StageStatus::Completed),
            "MergeConflict -> Completed should be a valid transition"
        );
    }

    #[test]
    fn test_merge_states_to_human_review_require_forced_assignment() {
        // O-3 escalation routes an exhausted-budget merge stage to
        // NeedsHumanReview. That edge is intentionally NOT in the transition
        // table (MergeConflict/MergeBlocked only legally go to Completed/Blocked
        // /Queued/Executing), so `escalate_merge_resolver_exhausted` MUST use
        // `force_status_with_reason`. If a future change makes this edge legal,
        // update the escalation path to prefer a `try_*` transition.
        use crate::models::stage::StageStatus;

        assert!(
            !StageStatus::MergeConflict.can_transition_to(&StageStatus::NeedsHumanReview),
            "MergeConflict -> NeedsHumanReview is expected to be illegal (escalation forces it)"
        );
        assert!(
            !StageStatus::MergeBlocked.can_transition_to(&StageStatus::NeedsHumanReview),
            "MergeBlocked -> NeedsHumanReview is expected to be illegal (escalation forces it)"
        );
    }

    #[test]
    fn test_plan_auto_merge_extraction_true() {
        let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  auto_merge: true
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
        let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
        let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
        assert_eq!(metadata.loom.auto_merge, Some(true));
    }

    #[test]
    fn test_plan_auto_merge_extraction_false() {
        let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  auto_merge: false
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
        let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
        let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
        assert_eq!(metadata.loom.auto_merge, Some(false));
    }

    #[test]
    fn test_plan_auto_merge_default_none() {
        let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
        let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
        let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
        assert_eq!(metadata.loom.auto_merge, None);
    }
}
