//! Merge status detection utilities for stages.
//!
//! This module provides utilities for detecting the merge state of completed stages,
//! including whether their work has been merged to the merge point, if there are
//! conflicts, or if branches are missing.

use anyhow::Result;
use std::path::Path;

use crate::git::branch::{branch_exists, branch_name_for_stage, is_ancestor_of};
use crate::models::stage::Stage;

/// The merge state of a completed stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeState {
    /// Stage work has been merged to the merge point
    Merged,
    /// Stage needs merge (work not yet in merge point)
    Pending,
    /// Stage has unresolved merge conflicts
    Conflict,
    /// Stage branch is missing (deleted without being marked as merged)
    BranchMissing,
    /// Cannot determine merge state (no completed_commit - legacy stage)
    Unknown,
}

impl std::fmt::Display for MergeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeState::Merged => write!(f, "Merged"),
            MergeState::Pending => write!(f, "Pending"),
            MergeState::Conflict => write!(f, "Conflict"),
            MergeState::BranchMissing => write!(f, "BranchMissing"),
            MergeState::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Check the merge state of a stage.
///
/// Determines whether a stage's work has been merged to the merge point
/// by examining the stage's metadata and git ancestry.
///
/// # Arguments
/// * `stage` - The stage to check
/// * `merge_point` - The branch/commit to check against (e.g., "main" or merge point SHA)
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// The merge state of the stage.
pub fn check_merge_state(stage: &Stage, merge_point: &str, repo_root: &Path) -> Result<MergeState> {
    // Check explicit conflict flag first
    if stage.merge_conflict {
        return Ok(MergeState::Conflict);
    }

    // Check if already marked as merged
    if stage.merged {
        return Ok(MergeState::Merged);
    }

    // Need completed_commit to check ancestry
    let Some(ref completed_commit) = stage.completed_commit else {
        return Ok(MergeState::Unknown);
    };

    // Check if the stage branch still exists
    let branch_name = branch_name_for_stage(&stage.id);
    let branch_exists = branch_exists(&branch_name, repo_root)?;

    if !branch_exists {
        // Branch is gone but stage not marked as merged - suspicious
        return Ok(MergeState::BranchMissing);
    }

    // Check if the completed commit is an ancestor of the merge point
    // If it is, the work has been merged
    match is_ancestor_of(completed_commit, merge_point, repo_root) {
        Ok(true) => Ok(MergeState::Merged),
        Ok(false) => Ok(MergeState::Pending),
        Err(_) => {
            // If we can't check ancestry (e.g., invalid refs), treat as unknown
            Ok(MergeState::Unknown)
        }
    }
}

/// Summary report of merge status across multiple stages.
#[derive(Debug, Default)]
pub struct MergeStatusReport {
    /// Stage IDs that have been merged
    pub merged: Vec<String>,
    /// Stage IDs pending merge
    pub pending: Vec<String>,
    /// Stage IDs with merge conflicts
    pub conflicts: Vec<String>,
    /// Warning messages (e.g., missing branches, unknown states)
    pub warnings: Vec<String>,
}

impl MergeStatusReport {
    /// Create a new empty report
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there are any issues requiring attention
    pub fn has_issues(&self) -> bool {
        !self.conflicts.is_empty() || !self.warnings.is_empty()
    }

    /// Check if all tracked stages have been merged
    pub fn all_merged(&self) -> bool {
        self.pending.is_empty() && self.conflicts.is_empty()
    }

    /// Total number of stages tracked
    pub fn total(&self) -> usize {
        self.merged.len() + self.pending.len() + self.conflicts.len()
    }
}

/// Build a merge status report for a collection of stages.
///
/// # Arguments
/// * `stages` - The stages to check
/// * `merge_point` - The branch/commit to check against
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// A report summarizing the merge status of all provided stages.
pub fn build_merge_report(
    stages: &[Stage],
    merge_point: &str,
    repo_root: &Path,
) -> Result<MergeStatusReport> {
    let mut report = MergeStatusReport::new();

    for stage in stages {
        // Only check completed stages
        if stage.status != crate::models::stage::StageStatus::Completed {
            continue;
        }

        match check_merge_state(stage, merge_point, repo_root) {
            Ok(MergeState::Merged) => {
                report.merged.push(stage.id.clone());
            }
            Ok(MergeState::Pending) => {
                report.pending.push(stage.id.clone());
            }
            Ok(MergeState::Conflict) => {
                report.conflicts.push(stage.id.clone());
            }
            Ok(MergeState::BranchMissing) => {
                report.warnings.push(format!(
                    "Stage '{}' branch missing but not marked as merged",
                    stage.id
                ));
            }
            Ok(MergeState::Unknown) => {
                report.warnings.push(format!(
                    "Stage '{}' has no completed_commit - cannot determine merge state",
                    stage.id
                ));
            }
            Err(e) => {
                report.warnings.push(format!(
                    "Failed to check merge state for '{}': {}",
                    stage.id, e
                ));
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Stage, StageStatus, StageType};
    use chrono::Utc;

    fn make_test_stage(id: &str) -> Stage {
        Stage {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            status: StageStatus::Completed,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            stage_type: StageType::default(),
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: Some(Utc::now()),
            started_at: None,
            duration_secs: None,
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
            verification_status: Default::default(),
            context_budget: None,
        }
    }

    #[test]
    fn test_merge_state_conflict_flag() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();

        let mut stage = make_test_stage("test-stage");
        stage.merge_conflict = true;

        let result = check_merge_state(&stage, "main", temp_dir.path()).unwrap();
        assert_eq!(result, MergeState::Conflict);
    }

    #[test]
    fn test_merge_state_merged_flag() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();

        let mut stage = make_test_stage("test-stage");
        stage.merged = true;

        let result = check_merge_state(&stage, "main", temp_dir.path()).unwrap();
        assert_eq!(result, MergeState::Merged);
    }

    #[test]
    fn test_merge_state_no_completed_commit() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();

        let stage = make_test_stage("test-stage");
        // completed_commit is None by default

        let result = check_merge_state(&stage, "main", temp_dir.path()).unwrap();
        assert_eq!(result, MergeState::Unknown);
    }

    #[test]
    fn test_merge_status_report_new() {
        let report = MergeStatusReport::new();
        assert!(report.merged.is_empty());
        assert!(report.pending.is_empty());
        assert!(report.conflicts.is_empty());
        assert!(report.warnings.is_empty());
        assert!(!report.has_issues());
        assert!(report.all_merged());
        assert_eq!(report.total(), 0);
    }

    #[test]
    fn test_merge_status_report_has_issues() {
        let mut report = MergeStatusReport::new();
        report.conflicts.push("stage-1".to_string());
        assert!(report.has_issues());
        assert!(!report.all_merged());
    }

    #[test]
    fn test_merge_status_report_all_merged() {
        let mut report = MergeStatusReport::new();
        report.merged.push("stage-1".to_string());
        report.merged.push("stage-2".to_string());
        assert!(report.all_merged());
        assert_eq!(report.total(), 2);
    }

    #[test]
    fn test_merge_state_display() {
        assert_eq!(format!("{}", MergeState::Merged), "Merged");
        assert_eq!(format!("{}", MergeState::Pending), "Pending");
        assert_eq!(format!("{}", MergeState::Conflict), "Conflict");
        assert_eq!(format!("{}", MergeState::BranchMissing), "BranchMissing");
        assert_eq!(format!("{}", MergeState::Unknown), "Unknown");
    }
}
