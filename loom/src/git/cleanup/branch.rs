//! Branch cleanup operations

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::git::branch::{branch_name_for_stage, delete_branch};

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
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            &format!("refs/heads/{branch_name}"),
        ])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to check branch existence")?;

    if !output.status.success() {
        // Branch doesn't exist
        return Ok(false);
    }

    // Delete the branch
    delete_branch(&branch_name, force, repo_root)?;
    Ok(true)
}
