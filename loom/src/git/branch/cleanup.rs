//! Branch cleanup operations

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use super::operations::delete_branch;

/// Clean up loom branches that have been merged
pub fn cleanup_merged_branches(target_branch: &str, repo_root: &Path) -> Result<Vec<String>> {
    // Get merged branches
    let output = Command::new("git")
        .args(["branch", "--merged", target_branch])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to get merged branches")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let merged_loom_branches: Vec<String> = stdout
        .lines()
        .map(|s| s.trim().trim_start_matches('*').trim().to_string())
        .filter(|s| s.starts_with("loom/"))
        .collect();

    let mut deleted = Vec::new();
    for branch in merged_loom_branches {
        if delete_branch(&branch, false, repo_root).is_ok() {
            deleted.push(branch);
        }
    }

    Ok(deleted)
}
