//! Automatic merge admission, execution and plan-level policy lookup.
use super::*;
use crate::orchestrator::auto_merge::{attempt_auto_merge, is_auto_merge_enabled};
use crate::verify::transitions::load_stage;

impl Orchestrator {
    /// Attempt auto-merge for a completed stage.
    ///
    /// Returns `true` if the merge succeeded or was not needed (stage can be marked Completed).
    /// Returns `false` if the merge did not land: the stage is `MergeConflict` (the spawn
    /// loop gives it a resolver), `MergeBlocked` (retried every tick) or in human review.
    pub(in crate::orchestrator::core) fn try_auto_merge(&mut self, stage_id: &str) -> bool {
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
        if self.missing_completion_proof(&stage, stage_id) {
            return false;
        }

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

    fn missing_completion_proof(
        &mut self,
        stage: &crate::models::stage::Stage,
        stage_id: &str,
    ) -> bool {
        if stage.completed_commit.is_none() {
            self.route_to_human_review(
                stage_id,
                "completion proof is missing; rerun loom stage complete before landing".into(),
                None,
            );
            return true;
        }
        false
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

        parse_plan_auto_merge(&plan_content, &plan_path, stage_id)
    }
}

fn parse_plan_auto_merge(
    plan_content: &str,
    plan_path: &std::path::Path,
    stage_id: &str,
) -> Option<bool> {
    let yaml_content = match crate::plan::parser::extract_yaml_metadata(plan_content) {
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
