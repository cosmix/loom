//! `loom stage merge <own> --resolved`, relayed from a Merge session.
//!
//! The daemon does what the CLI's `merge_resolved` does, with the daemon's
//! own phantom-merge guard: the stage must be in a merge-failed status, the
//! repository must hold no unmerged paths and no `MERGE_HEAD`, and
//! `finalize_merge_resolution` must prove by ancestry that the stage's commit
//! landed in the target before `merged = true` is written. Only then is the
//! worktree removed, through the same `MergeLifecycle` cleanup every merge
//! uses. Anything short of that leaves `merged = false` and says why.

use std::path::Path;

use crate::git::cleanup::CleanupConfig;
use crate::git::get_conflicting_files;
use crate::git::merge::merge_head_exists;
use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::merge_lifecycle::{finish_verified_merge, CleanupOutcome};

use super::super::merge_handler::report_deferred_cleanup;
use super::super::persistence::Persistence;
use super::super::Orchestrator;
use super::Settle;

impl Orchestrator {
    /// Finalize the merge `session` resolved for `stage_id`.
    pub(super) fn resolve_merge_from_inbox(&mut self, session: &Session, stage_id: &str) -> Settle {
        let mut stage = match self.load_stage(stage_id) {
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
        if let Err(reason) = merge_is_concluded(&self.config.repo_root) {
            return Settle::Refused(reason);
        }
        let merge_point = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        let message = "merge resolution relayed and verified";
        if !self.finalize_merge_resolution(&mut stage, &session.id, stage_id, &merge_point, message)
        {
            return Settle::Refused(format!(
                "no ancestry proof that stage '{stage_id}' landed in '{merge_point}'; merged stays false"
            ));
        }
        let outcome = finish_verified_merge(
            stage_id,
            &self.config.repo_root,
            &self.config.work_dir,
            &merge_point,
            &CleanupConfig::quiet(),
        );
        report_deferred_cleanup(stage_id, &outcome);
        Settle::Applied(Some(format!(
            "merged into '{merge_point}'; worktree cleanup {}",
            describe(&outcome)
        )))
    }
}

/// The resolution is committed: no unmerged paths and no merge in progress.
fn merge_is_concluded(repo_root: &Path) -> Result<(), String> {
    match get_conflicting_files(repo_root) {
        Ok(files) if files.is_empty() => {}
        Ok(files) => return Err(format!("unmerged paths remain: {}", files.join(", "))),
        Err(error) => return Err(format!("could not list unmerged paths: {error:#}")),
    }
    match merge_head_exists(repo_root) {
        Ok(false) => Ok(()),
        Ok(true) => {
            Err("a merge is still in progress (MERGE_HEAD exists); commit it first".to_string())
        }
        Err(error) => Err(format!("could not check for MERGE_HEAD: {error:#}")),
    }
}

fn describe(outcome: &CleanupOutcome) -> String {
    match outcome {
        CleanupOutcome::Done(_) => "done".to_string(),
        CleanupOutcome::NothingToDo => "had nothing to remove".to_string(),
        CleanupOutcome::Deferred => "deferred".to_string(),
        CleanupOutcome::Refused { reason } => format!("refused: {reason}"),
        CleanupOutcome::Failed(error) => format!("failed: {error}"),
    }
}
