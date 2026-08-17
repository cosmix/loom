//! Auto-merge service for automatic branch merging on stage completion
//!
//! This module provides functionality to automatically merge stage branches
//! when stages reach the Completed status. It integrates with the existing
//! merge infrastructure and can spawn conflict resolution sessions when needed.

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::merge::{merge_stage, MergeResult};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::signals::generate_merge_signal;
use crate::orchestrator::terminal::backend::SessionBackend;

/// Result of an auto-merge attempt.
///
/// No variant carries a cleanup result: `attempt_auto_merge` merges and
/// reports, and cleanup belongs to the caller via
/// `crate::orchestrator::merge_lifecycle`, after ancestry has been verified.
#[derive(Debug)]
pub enum AutoMergeResult {
    /// Merge completed successfully
    Success {
        files_changed: u32,
        insertions: u32,
        deletions: u32,
    },
    /// Fast-forward merge completed
    FastForward,
    /// Already up to date (no changes needed)
    AlreadyUpToDate,
    /// Conflicts detected, spawned resolution session.
    /// Boxed to keep the enum compact — `Session` carries runtime-identity
    /// fields (`tracking_key`) and dwarfs other variants.
    ConflictResolutionSpawned {
        session: Box<Session>,
        conflicting_files: Vec<String>,
    },
    /// Stage has no worktree (nothing to merge)
    NoWorktree,
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
/// 2. Attempts to merge the stage branch to the target branch
/// 3. On success: reports the merge statistics and stops there
/// 4. On conflict: spawns a Claude Code session for resolution
///
/// Cleanup is deliberately NOT done here. It belongs to the caller, via
/// `crate::orchestrator::merge_lifecycle`, and runs only after the merge has
/// been verified by git ancestry. Removing the worktree and branch inside this
/// function destroyed the very evidence the caller needs: the daemon derives a
/// missing `completed_commit` from the stage branch HEAD, which cleanup had
/// already deleted.
///
/// Note: This function does not print any output. The caller is responsible
/// for logging or displaying results based on the returned `AutoMergeResult`.
pub fn attempt_auto_merge(
    stage: &Stage,
    repo_root: &Path,
    work_dir: &Path,
    target_branch: &str,
    backend: &SessionBackend,
) -> Result<AutoMergeResult> {
    // Check if stage has a worktree
    let worktree_path = repo_root.join(".worktrees").join(&stage.id);
    if !worktree_path.exists() {
        return Ok(AutoMergeResult::NoWorktree);
    }

    // Attempt the merge
    let merge_result =
        merge_stage(&stage.id, target_branch, repo_root, work_dir).context("Auto-merge failed")?;

    match merge_result {
        MergeResult::Success {
            files_changed,
            insertions,
            deletions,
        } => Ok(AutoMergeResult::Success {
            files_changed,
            insertions,
            deletions,
        }),

        MergeResult::FastForward => Ok(AutoMergeResult::FastForward),

        MergeResult::AlreadyUpToDate => Ok(AutoMergeResult::AlreadyUpToDate),

        MergeResult::Conflict { conflicting_files } => {
            // Create a merge session to resolve conflicts
            let source_branch = branch_name_for_stage(&stage.id);
            let session = Session::new_merge(source_branch.clone(), target_branch.to_string());

            // Generate the merge signal file. Fresh conflict path: no
            // in-progress merge to inherit (the test merge in merge_stage was
            // aborted before returning Conflict).
            let signal_path = generate_merge_signal(
                &session,
                stage,
                &source_branch,
                target_branch,
                &conflicting_files,
                None,
                work_dir,
            )
            .context("Failed to generate merge signal")?;

            // Spawn the merge resolution session.
            let spawned_session = backend
                .spawn_merge_session(stage, session, &signal_path, repo_root)
                .context("Failed to spawn merge resolution session")?;

            Ok(AutoMergeResult::ConflictResolutionSpawned {
                session: Box::new(spawned_session),
                conflicting_files,
            })
        }
    }
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
