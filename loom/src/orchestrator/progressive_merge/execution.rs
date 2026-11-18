//! Merge execution logic for progressive merging

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::work_dir::WorkDir;
use crate::git::branch::{branch_exists, branch_name_for_stage};
use crate::git::merge::{merge_stage, MergeGate, MergeResult};
use crate::models::stage::Stage;

use super::ProgressiveMergeResult;

/// Attempt to merge a just-completed stage into the merge point.
///
/// Called immediately after verification passes. This function:
/// 1. Checks if the stage's branch exists
/// 2. Attempts to merge the branch under the lock owned by `merge_stage`
/// 3. Returns the result (success, conflict, or no-op)
///
/// # Arguments
/// * `stage` - The stage that just completed verification
/// * `repo_root` - Path to the repository root
/// * `merge_point` - Target branch to merge into (usually "main" or a staging branch)
/// * `gate` - Whether the merge enforces the control-path gate
///
/// # Returns
/// * `Ok(ProgressiveMergeResult::Success)` - Branch merged successfully
/// * `Ok(ProgressiveMergeResult::AlreadyMerged)` - No changes to merge
/// * `Ok(ProgressiveMergeResult::Conflict)` - Conflicts detected, stage needs resolution
/// * `Ok(ProgressiveMergeResult::Blocked)` - The merge did not advance for an operator-clearable reason
/// * `Ok(ProgressiveMergeResult::Held)` - `gate` is `Enforce` and the branch touches a control path
/// * `Ok(ProgressiveMergeResult::NoBranch)` - Branch doesn't exist (already cleaned up)
/// * `Err(_)` - Unexpected error during merge
pub fn merge_completed_stage(
    stage: &Stage,
    repo_root: &Path,
    merge_point: &str,
    gate: MergeGate,
) -> Result<ProgressiveMergeResult> {
    let branch_name = branch_name_for_stage(&stage.id);

    // Check if branch exists before trying to merge
    if !branch_exists(&branch_name, repo_root)? {
        return Ok(ProgressiveMergeResult::NoBranch);
    }

    // Get the work directory - merge_stage will acquire the lock
    let work_dir = WorkDir::new(repo_root)?;
    if !work_dir.root().exists() {
        return Err(anyhow::anyhow!(".loom/work directory not found"));
    }

    // Attempt the merge (merge_stage will acquire the lock internally)
    let result = merge_stage(&stage.id, merge_point, repo_root, work_dir.root(), gate)
        .with_context(|| format!("Failed to merge stage {} into {}", stage.id, merge_point))?;

    // Convert git::merge::MergeResult to ProgressiveMergeResult
    let progressive_result = match result {
        MergeResult::Success {
            files_changed,
            stash,
            ..
        } => ProgressiveMergeResult::Success {
            files_changed,
            stash,
        },
        MergeResult::Blocked(block) => ProgressiveMergeResult::Blocked(block),
        MergeResult::Held { reason } => ProgressiveMergeResult::Held { reason },
        MergeResult::AlreadyUpToDate => ProgressiveMergeResult::AlreadyMerged,
        MergeResult::Conflict { conflicting_files } => {
            ProgressiveMergeResult::Conflict { conflicting_files }
        }
    };

    Ok(progressive_result)
}

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
