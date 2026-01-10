//! Auto-merge service for automatic branch merging on stage completion
//!
//! This module provides functionality to automatically merge stage branches
//! when stages reach the Completed status. It integrates with the existing
//! merge infrastructure and can spawn conflict resolution sessions when needed.

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::cleanup::{cleanup_after_merge, CleanupConfig, CleanupResult};
use crate::git::merge::{merge_stage, MergeResult};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::signals::generate_merge_signal;
use crate::orchestrator::terminal::TerminalBackend;

/// Result of an auto-merge attempt
#[derive(Debug)]
pub enum AutoMergeResult {
    /// Merge completed successfully
    Success {
        files_changed: u32,
        insertions: u32,
        deletions: u32,
        cleanup: CleanupResult,
    },
    /// Fast-forward merge completed
    FastForward { cleanup: CleanupResult },
    /// Already up to date (no changes needed)
    AlreadyUpToDate { cleanup: CleanupResult },
    /// Conflicts detected, spawned resolution session
    ConflictResolutionSpawned {
        session_id: String,
        conflicting_files: Vec<String>,
    },
    /// Stage has no worktree (nothing to merge)
    NoWorktree,
    /// Auto-merge is disabled for this stage
    Disabled,
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
/// 3. On success: cleans up the worktree and branch
/// 4. On conflict: spawns a Claude Code session for resolution
///
/// Note: This function does not print any output. The caller is responsible
/// for logging or displaying results based on the returned `AutoMergeResult`.
pub fn attempt_auto_merge(
    stage: &Stage,
    repo_root: &Path,
    work_dir: &Path,
    target_branch: &str,
    backend: &dyn TerminalBackend,
) -> Result<AutoMergeResult> {
    // Check if stage has a worktree
    let worktree_path = repo_root.join(".worktrees").join(&stage.id);
    if !worktree_path.exists() {
        return Ok(AutoMergeResult::NoWorktree);
    }

    // Attempt the merge
    let merge_result =
        merge_stage(&stage.id, target_branch, repo_root).context("Auto-merge failed")?;

    match merge_result {
        MergeResult::Success {
            files_changed,
            insertions,
            deletions,
        } => {
            // Clean up worktree and branch
            let cleanup = cleanup_after_merge(&stage.id, repo_root, &CleanupConfig::quiet())?;

            Ok(AutoMergeResult::Success {
                files_changed,
                insertions,
                deletions,
                cleanup,
            })
        }

        MergeResult::FastForward => {
            let cleanup = cleanup_after_merge(&stage.id, repo_root, &CleanupConfig::quiet())?;

            Ok(AutoMergeResult::FastForward { cleanup })
        }

        MergeResult::AlreadyUpToDate => {
            let cleanup = cleanup_after_merge(&stage.id, repo_root, &CleanupConfig::quiet())?;

            Ok(AutoMergeResult::AlreadyUpToDate { cleanup })
        }

        MergeResult::Conflict { conflicting_files } => {
            // Create a merge session to resolve conflicts
            let source_branch = format!("loom/{}", stage.id);
            let session = Session::new_merge(source_branch.clone(), target_branch.to_string());

            // Generate the merge signal file
            let signal_path = generate_merge_signal(
                &session,
                stage,
                &source_branch,
                target_branch,
                &conflicting_files,
                work_dir,
            )
            .context("Failed to generate merge signal")?;

            // Spawn the merge resolution session
            let spawned_session = backend
                .spawn_merge_session(stage, session, &signal_path, repo_root)
                .context("Failed to spawn merge resolution session")?;

            Ok(AutoMergeResult::ConflictResolutionSpawned {
                session_id: spawned_session.id,
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
            description: None,
            status: StageStatus::Completed,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            plan_id: None,
            worktree: Some(id.to_string()),
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: Some(Utc::now()),
            close_reason: None,
            auto_merge: None,
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
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
