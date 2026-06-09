//! Progressive merge service for immediate branch merging after verification
//!
//! This module provides functionality to merge stage branches immediately after
//! verification passes. This is the core of conflict prevention - by merging
//! verified branches as soon as they pass, we minimize the window for conflicts.
//!
//! The merge uses file-based locking to prevent concurrent merges from multiple
//! stages completing simultaneously.

pub mod execution;

pub use crate::fs::get_merge_point;
pub use crate::git::merge::lock::MergeLock;
pub use execution::{merge_completed_stage, merge_completed_stage_with_timeout};

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
    /// Returns true if the merge succeeded (the stage's work is provably in the
    /// target branch).
    ///
    /// `NoBranch` is deliberately NOT a success: a missing stage branch means
    /// the work was never committed (or was lost), not that it landed. Counting
    /// it as success would let a stage be treated as merged with no ancestry
    /// proof — the phantom-merge bug (A-3/O-2). Callers must route `NoBranch`
    /// through the same surface-to-user / blocked path as a conflict.
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            ProgressiveMergeResult::Success { .. }
                | ProgressiveMergeResult::FastForward
                | ProgressiveMergeResult::AlreadyMerged
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
            status: StageStatus::Completed,
            worktree: Some(id.to_string()),
            completed_at: Some(Utc::now()),
            ..Stage::default()
        }
    }

    #[test]
    fn test_progressive_merge_result_is_success() {
        assert!(ProgressiveMergeResult::Success { files_changed: 5 }.is_success());
        assert!(ProgressiveMergeResult::FastForward.is_success());
        assert!(ProgressiveMergeResult::AlreadyMerged.is_success());
        // NoBranch is NOT success: a missing branch means the work was never
        // committed, not that it merged (phantom-merge prevention, A-3/O-2).
        assert!(!ProgressiveMergeResult::NoBranch.is_success());
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
