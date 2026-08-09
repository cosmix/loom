//! Branch cleanup operations

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::{branch_name_for_stage, delete_branch};
use crate::git::runner::run_git;

/// Clean up the branch for a stage
///
/// # Arguments
/// * `stage_id` - The stage ID whose branch to delete
/// * `repo_root` - Path to the repository root
/// * `force` - Force deletion even if not fully merged
///
/// # Returns
/// `true` if the branch was deleted, `false` if it didn't exist
pub fn cleanup_branch(stage_id: &str, repo_root: &Path, force: bool) -> Result<bool> {
    let branch_name = branch_name_for_stage(stage_id);

    // Check if branch exists first
    if !branch_exists_strict(&branch_name, repo_root)? {
        return Ok(false);
    }

    // Delete the branch
    delete_branch(&branch_name, force, repo_root)?;
    Ok(true)
}

pub(crate) fn branch_exists_strict(branch_name: &str, repo_root: &Path) -> Result<bool> {
    let reference = format!("refs/heads/{branch_name}");
    let output = run_git(&["show-ref", "--verify", "--quiet", &reference], repo_root)
        .with_context(|| format!("Failed to check branch '{branch_name}' existence"))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        code => anyhow::bail!(
            "git show-ref failed while checking branch '{branch_name}' (exit {}): {}",
            code.map(|value| value.to_string())
                .unwrap_or_else(|| "signal".to_string()),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}
