//! Auto-merge service for automatic branch merging on stage completion
//!
//! This module provides functionality to automatically merge stage branches
//! when stages reach the Completed status. It integrates with the existing
//! merge infrastructure; a conflict is reported to the caller, which hands the
//! stage to the daemon's resolver spawn loop.

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::merge::{merge_stage, MergeBlock, MergeGate, MergeResult, StashReapply};
use crate::models::stage::Stage;

/// Result of an auto-merge attempt.
///
/// No variant carries a cleanup result: `attempt_auto_merge` merges and
/// reports, and cleanup belongs to the caller via
/// `crate::orchestrator::merge_lifecycle`, after ancestry has been verified.
#[derive(Debug)]
pub enum AutoMergeResult {
    /// The target branch now holds the stage's merge commit.
    Success {
        files_changed: u32,
        insertions: u32,
        deletions: u32,
        /// The operator's stashed changes, when they were stashed around
        /// the merge.
        stash: Option<StashReapply>,
    },
    /// Already up to date (no changes needed)
    AlreadyUpToDate,
    /// The merge conflicts; nothing was changed. The caller moves the stage
    /// to `MergeConflict` and the spawn loop gives it a resolver.
    Conflict { conflicting_files: Vec<String> },
    /// The target was not advanced and nothing in the main checkout changed;
    /// the caller records the reason and retries.
    Blocked(MergeBlock),
    /// The control-path gate refused the branch; nothing was changed. The
    /// caller routes the stage to human review with `reason`.
    Held { reason: String },
    /// Stage has no worktree (nothing to merge)
    NoWorktree,
}

impl From<AutoMergeResult> for crate::stage_service::merge_outcome::MergeDisposition {
    fn from(result: AutoMergeResult) -> Self {
        match result {
            AutoMergeResult::Success {
                files_changed,
                insertions,
                deletions,
                stash,
            } => Self::Landed {
                files: files_changed,
                insertions,
                deletions,
                stash,
            },
            AutoMergeResult::AlreadyUpToDate => Self::AlreadyLanded,
            AutoMergeResult::Conflict { conflicting_files } => Self::Conflict(conflicting_files),
            AutoMergeResult::Blocked(block) => Self::Blocked(block),
            AutoMergeResult::Held { reason } => Self::Held(reason),
            AutoMergeResult::NoWorktree => Self::MissingWorktree,
        }
    }
}

/// Check if auto-merge is enabled for a stage
///
/// Priority (highest to lowest):
/// 1. Stage-level `auto_merge` setting
/// 2. Plan-level `auto_merge` setting
/// 3. Orchestrator config `auto_merge` setting
pub fn is_auto_merge_enabled(
    stage: &Stage,
    orchestrator_auto_merge: bool,
    plan_auto_merge: Option<bool>,
) -> bool {
    stage
        .auto_merge
        .or(plan_auto_merge)
        .unwrap_or(orchestrator_auto_merge)
}

/// Attempt to auto-merge a completed stage
///
/// This function:
/// 1. Checks if the stage has a worktree
/// 2. Merges the stage branch into the target branch with `merge_stage`
/// 3. Maps the result; a conflict spawns nothing here
///
/// Cleanup is deliberately NOT done here. It belongs to the caller, via
/// `crate::orchestrator::merge_lifecycle`, and runs only after the merge has
/// been verified against certified completion proof by git ancestry. A missing
/// `completed_commit` is refused rather than inferred from the branch head.
///
/// Note: This function does not print any output. The caller is responsible
/// for logging or displaying results based on the returned `AutoMergeResult`.
pub fn attempt_auto_merge(
    stage: &Stage,
    repo_root: &Path,
    work_dir: &Path,
    target_branch: &str,
) -> Result<AutoMergeResult> {
    let worktree_path = repo_root.join(".worktrees").join(&stage.id);
    if !worktree_path.exists() {
        return Ok(AutoMergeResult::NoWorktree);
    }

    let merge_result = merge_stage(
        &stage.id,
        target_branch,
        repo_root,
        work_dir,
        MergeGate::Enforce,
    )
    .context("Auto-merge failed")?;
    Ok(match merge_result {
        MergeResult::Success {
            files_changed,
            insertions,
            deletions,
            stash,
        } => AutoMergeResult::Success {
            files_changed,
            insertions,
            deletions,
            stash,
        },
        MergeResult::AlreadyUpToDate => AutoMergeResult::AlreadyUpToDate,
        MergeResult::Conflict { conflicting_files } => {
            AutoMergeResult::Conflict { conflicting_files }
        }
        MergeResult::Blocked(block) => AutoMergeResult::Blocked(block),
        MergeResult::Held { reason } => AutoMergeResult::Held { reason },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use chrono::Utc;

    fn create_test_stage(id: &str) -> Stage {
        Stage {
            id: id.to_string(),
            name: format!("Test Stage {id}"),
            status: StageStatus::Completed,
            worktree: Some(id.to_string()),
            completed_at: Some(Utc::now()),
            ..Stage::default()
        }
    }

    #[test]
    fn test_is_auto_merge_enabled_stage_override() {
        let mut stage = create_test_stage("test-1");

        // Stage override takes precedence
        stage.auto_merge = Some(true);
        assert!(is_auto_merge_enabled(&stage, false, None));
        assert!(is_auto_merge_enabled(&stage, false, Some(false)));

        stage.auto_merge = Some(false);
        assert!(!is_auto_merge_enabled(&stage, true, Some(true)));
    }

    #[test]
    fn test_is_auto_merge_enabled_plan_override() {
        let mut stage = create_test_stage("test-1");
        stage.auto_merge = None;

        // Plan override takes precedence over orchestrator
        assert!(is_auto_merge_enabled(&stage, false, Some(true)));
        assert!(!is_auto_merge_enabled(&stage, true, Some(false)));
    }

    #[test]
    fn test_is_auto_merge_enabled_orchestrator_default() {
        let mut stage = create_test_stage("test-1");
        stage.auto_merge = None;

        // Falls back to orchestrator config when no overrides
        assert!(is_auto_merge_enabled(&stage, true, None));
        assert!(!is_auto_merge_enabled(&stage, false, None));
    }

    #[test]
    fn test_is_auto_merge_enabled_priority() {
        let mut stage = create_test_stage("test-1");

        // Test full priority chain: stage > plan > orchestrator
        stage.auto_merge = Some(true);
        assert!(is_auto_merge_enabled(&stage, false, Some(false)));

        stage.auto_merge = None;
        assert!(!is_auto_merge_enabled(&stage, true, Some(false)));

        stage.auto_merge = None;
        assert!(is_auto_merge_enabled(&stage, true, None));
    }
}
