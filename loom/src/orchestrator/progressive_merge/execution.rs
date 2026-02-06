//! Merge execution logic for progressive merging

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;

use crate::git::branch::{branch_exists, branch_name_for_stage};
use crate::git::merge::{merge_stage, MergeResult};
use crate::models::stage::Stage;

use super::lock::MergeLock;
use super::ProgressiveMergeResult;

/// Attempt to merge a just-completed stage into the merge point.
///
/// Called immediately after verification passes. This function:
/// 1. Acquires a file-based lock to prevent concurrent merges
/// 2. Checks if the stage's branch exists
/// 3. Attempts to merge the branch into the merge point
/// 4. Returns the result (success, conflict, or no-op)
///
/// # Arguments
/// * `stage` - The stage that just completed verification
/// * `repo_root` - Path to the repository root
/// * `merge_point` - Target branch to merge into (usually "main" or a staging branch)
///
/// # Returns
/// * `Ok(ProgressiveMergeResult::Success)` - Branch merged successfully
/// * `Ok(ProgressiveMergeResult::FastForward)` - Fast-forward merge completed
/// * `Ok(ProgressiveMergeResult::AlreadyMerged)` - No changes to merge
/// * `Ok(ProgressiveMergeResult::Conflict)` - Conflicts detected, stage needs resolution
/// * `Ok(ProgressiveMergeResult::NoBranch)` - Branch doesn't exist (already cleaned up)
/// * `Err(_)` - Unexpected error during merge
pub fn merge_completed_stage(
    stage: &Stage,
    repo_root: &Path,
    merge_point: &str,
) -> Result<ProgressiveMergeResult> {
    merge_completed_stage_with_timeout(stage, repo_root, merge_point, Duration::from_secs(30))
}

/// Attempt to merge with a custom lock timeout
pub fn merge_completed_stage_with_timeout(
    stage: &Stage,
    repo_root: &Path,
    merge_point: &str,
    lock_timeout: Duration,
) -> Result<ProgressiveMergeResult> {
    let branch_name = branch_name_for_stage(&stage.id);

    // Check if branch exists before trying to merge
    if !branch_exists(&branch_name, repo_root)? {
        return Ok(ProgressiveMergeResult::NoBranch);
    }

    // Get the work directory for locking
    let work_dir = repo_root.join(".work");
    if !work_dir.exists() {
        return Err(anyhow::anyhow!(".work directory not found"));
    }

    // Acquire merge lock to prevent concurrent merges
    let _lock = MergeLock::acquire(&work_dir, lock_timeout)
        .context("Failed to acquire merge lock - another merge may be in progress")?;

    // Attempt the merge
    let result = merge_stage(&stage.id, merge_point, repo_root)
        .with_context(|| format!("Failed to merge stage {} into {}", stage.id, merge_point))?;

    // Convert git::merge::MergeResult to ProgressiveMergeResult
    let progressive_result = match result {
        MergeResult::Success { files_changed, .. } => {
            ProgressiveMergeResult::Success { files_changed }
        }
        MergeResult::FastForward => ProgressiveMergeResult::FastForward,
        MergeResult::AlreadyUpToDate => ProgressiveMergeResult::AlreadyMerged,
        MergeResult::Conflict { conflicting_files } => {
            ProgressiveMergeResult::Conflict { conflicting_files }
        }
    };

    // Lock is automatically released when _lock goes out of scope
    Ok(progressive_result)
}

// get_merge_point is now provided by crate::fs module

#[cfg(test)]
mod tests {
    use crate::fs::get_merge_point;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_get_merge_point_default() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // No config.toml - should return "main"
        let result = get_merge_point(work_dir).unwrap();
        assert_eq!(result, "main");
    }

    #[test]
    fn test_get_merge_point_from_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create config.toml with custom base_branch
        let config_content = r#"
[plan]
source_path = "doc/plans/test.md"
plan_id = "test"
base_branch = "develop"
"#;
        fs::write(work_dir.join("config.toml"), config_content).unwrap();

        let result = get_merge_point(work_dir).unwrap();
        assert_eq!(result, "develop");
    }

    #[test]
    fn test_get_merge_point_missing_base_branch() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create config.toml without base_branch
        let config_content = r#"
[plan]
source_path = "doc/plans/test.md"
plan_id = "test"
"#;
        fs::write(work_dir.join("config.toml"), config_content).unwrap();

        let result = get_merge_point(work_dir).unwrap();
        assert_eq!(result, "main");
    }
}
