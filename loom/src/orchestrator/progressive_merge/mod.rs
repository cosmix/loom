//! Progressive merge service for immediate branch merging after verification
//!
//! This module provides functionality to merge stage branches immediately after
//! verification passes. This is the core of conflict prevention - by merging
//! verified branches as soon as they pass, we minimize the window for conflicts.
//!
//! The merge uses file-based locking to prevent concurrent merges from multiple
//! stages completing simultaneously.

pub mod execution;
pub mod lock;

pub use execution::{get_merge_point, merge_completed_stage, merge_completed_stage_with_timeout};
pub use lock::MergeLock;

/// Result of a progressive merge attempt
#[derive(Debug, Clone)]
pub enum ProgressiveMergeResult {
    /// Merge completed successfully with changes
    Success { files_changed: u32 },
    /// Fast-forward merge completed (no merge commit needed)
    FastForward,
    /// Branch was already merged or up to date
    AlreadyMerged,
    /// Conflicts detected that need resolution
    Conflict { conflicting_files: Vec<String> },
    /// Branch doesn't exist (already cleaned up)
    NoBranch,
}

impl ProgressiveMergeResult {
    /// Returns true if the merge succeeded (no conflicts)
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            ProgressiveMergeResult::Success { .. }
                | ProgressiveMergeResult::FastForward
                | ProgressiveMergeResult::AlreadyMerged
                | ProgressiveMergeResult::NoBranch
        )
    }

    /// Returns the conflicting files if there was a conflict
    pub fn conflicting_files(&self) -> Option<&[String]> {
        match self {
            ProgressiveMergeResult::Conflict { conflicting_files } => Some(conflicting_files),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Stage, StageStatus};
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
            stage_type: crate::models::stage::StageType::default(),
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
            working_dir: Some(".".to_string()),
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
    fn test_progressive_merge_result_is_success() {
        assert!(ProgressiveMergeResult::Success { files_changed: 5 }.is_success());
        assert!(ProgressiveMergeResult::FastForward.is_success());
        assert!(ProgressiveMergeResult::AlreadyMerged.is_success());
        assert!(ProgressiveMergeResult::NoBranch.is_success());
        assert!(!ProgressiveMergeResult::Conflict {
            conflicting_files: vec!["file.rs".to_string()]
        }
        .is_success());
    }

    #[test]
    fn test_progressive_merge_result_conflicting_files() {
        let conflict = ProgressiveMergeResult::Conflict {
            conflicting_files: vec!["a.rs".to_string(), "b.rs".to_string()],
        };
        assert_eq!(
            conflict.conflicting_files(),
            Some(&["a.rs".to_string(), "b.rs".to_string()][..])
        );

        assert!(ProgressiveMergeResult::Success { files_changed: 1 }
            .conflicting_files()
            .is_none());
    }

    #[test]
    fn test_stage_fields_exist() {
        // Verify the Stage model has the merge tracking fields we need
        let mut stage = create_test_stage("test");
        assert!(!stage.merged);
        assert!(!stage.merge_conflict);

        // Should be able to update these fields
        stage.merged = true;
        stage.merge_conflict = true;
        assert!(stage.merged);
        assert!(stage.merge_conflict);
    }
}
