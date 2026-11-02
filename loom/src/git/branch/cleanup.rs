//! Branch cleanup operations

use anyhow::Result;
use std::path::Path;

use super::operations::delete_branch;

/// Clean up loom branches that have been merged
pub fn cleanup_merged_branches(target_branch: &str, repo_root: &Path) -> Result<Vec<String>> {
    let merged_loom_branches = super::info::list_refs(repo_root, Some(target_branch))?
        .into_iter()
        .map(|branch| branch.name)
        .filter(|name| name.starts_with("loom/"));

    let mut deleted = Vec::new();
    for branch in merged_loom_branches {
        if delete_branch(&branch, false, repo_root).is_ok() {
            deleted.push(branch);
        }
    }

    Ok(deleted)
}
