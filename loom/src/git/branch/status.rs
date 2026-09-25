//! Git status checking for uncommitted changes

use anyhow::{bail, Result};
use std::path::Path;

use crate::git::runner::run_git;
use crate::verify::tool_artifacts::is_tool_artifact;

/// Check if the repository has uncommitted changes (staged or unstaged)
///
/// Uses `git status --porcelain` to detect:
/// - Staged but uncommitted changes (index)
/// - Unstaged modifications in working tree
/// - Untracked files are NOT considered (they don't affect worktree creation)
///
/// # Arguments
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// * `Ok(true)` if there are uncommitted changes
/// * `Ok(false)` if the working tree is clean (no staged/unstaged changes)
/// * `Err` if git command fails
pub fn has_uncommitted_changes(repo_root: &Path) -> Result<bool> {
    let output = run_git(&["status", "--porcelain"], repo_root)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check for staged or modified files (exclude untracked with ??)
    let has_changes = stdout.lines().any(|line| {
        // Porcelain format: XY filename
        // X = index status, Y = worktree status
        // ?? = untracked (ignore these)
        !line.starts_with("??") && !line.is_empty()
    });

    Ok(has_changes)
}

/// List every locally changed path in the working tree
///
/// Unlike [`has_uncommitted_changes`], untracked files ARE included — a new
/// module an agent added is untracked, and that is exactly the case callers
/// asking "has work happened here?" care about. Files ignored by `.gitignore` /
/// `.git/info/exclude` are excluded by git itself, and untracked sandbox
/// artifacts at the root by [`is_tool_artifact`].
///
/// Paths are as git reports them, relative to the repository root; untracked
/// directories are reported collapsed (`some/dir/`). For renames only the
/// destination path is returned.
///
/// # Arguments
/// * `repo_root` - Path to the git repository or worktree root
///
/// # Returns
/// * `Ok(paths)` - changed paths, empty when the working tree is pristine
/// * `Err` if the git command fails
pub fn list_working_tree_changes(repo_root: &Path) -> Result<Vec<String>> {
    let output = run_git(&["status", "--porcelain"], repo_root)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(working_tree_changes(repo_root, &stdout))
}

/// The paths in `git status --porcelain` output, as
/// [`list_working_tree_changes`] reports them.
fn working_tree_changes(repo_root: &Path, porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter(|line| line.len() > 3)
        .filter(|line| !(line.starts_with("??") && is_tool_artifact(repo_root, &line[3..])))
        // Porcelain v1: "XY path" — or "XY old -> new" for renames/copies.
        .map(|line| match line[3..].split_once(" -> ") {
            Some((_, destination)) => destination.to_string(),
            None => line[3..].to_string(),
        })
        .collect()
}

/// Get a summary of uncommitted changes for display
///
/// Returns a human-readable summary of staged and unstaged changes.
///
/// # Arguments
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// * `Ok(summary)` - A string describing the changes, empty if clean
/// * `Err` if git command fails
pub fn get_uncommitted_changes_summary(repo_root: &Path) -> Result<String> {
    let output = run_git(&["status", "--porcelain"], repo_root)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut staged = Vec::new();
    let mut modified = Vec::new();

    for line in stdout.lines() {
        if line.is_empty() || line.starts_with("??") {
            continue;
        }

        // Porcelain format: XY filename
        let chars: Vec<char> = line.chars().collect();
        if chars.len() < 3 {
            continue;
        }

        let index_status = chars[0];
        let worktree_status = chars[1];
        let filename = line[3..].to_string();

        // X != ' ' means staged
        if index_status != ' ' && index_status != '?' {
            staged.push(filename.clone());
        }
        // Y != ' ' means modified in worktree
        if worktree_status != ' ' && worktree_status != '?' {
            modified.push(filename);
        }
    }

    let mut summary = String::new();
    if !staged.is_empty() {
        summary.push_str(&format!("Staged: {}\n", staged.join(", ")));
    }
    if !modified.is_empty() {
        summary.push_str(&format!("Modified: {}\n", modified.join(", ")));
    }

    Ok(summary)
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
