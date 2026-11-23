//! Git status checking for uncommitted changes

use anyhow::{bail, Result};
use std::path::Path;

use crate::git::runner::run_git;
use crate::git::status_entries::{parse_porcelain_v1_z, StatusEntry};
use crate::git::worktree::WorktreeGit;
use crate::verify::tool_artifacts::is_tool_artifact;

/// `git status` as a NUL-separated porcelain v1 listing, untracked
/// directories collapsed (git's default untracked mode).
const STATUS_Z: [&str; 3] = ["status", "--porcelain=v1", "-z"];

/// Check if the repository has uncommitted changes (staged or unstaged)
///
/// Uses `git status --porcelain=v1 -z` to detect:
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
/// * `Err` if git command fails or prints a listing that does not parse
pub fn has_uncommitted_changes(repo_root: &Path) -> Result<bool> {
    Ok(is_dirty(&status_entries(repo_root)?))
}

/// List every locally changed path in the working tree
///
/// Unlike [`has_uncommitted_changes`], untracked files ARE included — a new
/// module an agent added is untracked, and that is exactly the case callers
/// asking "has work happened here?" care about. Files ignored by `.gitignore` /
/// `.git/info/exclude` are excluded by git itself, and untracked sandbox
/// artifacts by [`is_tool_artifact`].
///
/// Paths are as git names them, relative to the repository root and never
/// C-quoted; untracked directories are reported collapsed (`some/dir/`). For
/// renames only the destination path is returned.
///
/// # Arguments
/// * `repo_root` - Path to the git repository or worktree root
///
/// # Returns
/// * `Ok(paths)` - changed paths, empty when the working tree is pristine
/// * `Err` if the git command fails or prints a listing that does not parse
pub fn list_working_tree_changes(repo_root: &Path) -> Result<Vec<String>> {
    let status = WorktreeGit::pinned_checkout(repo_root)?.run_checked(&STATUS_Z)?;
    working_tree_changes(repo_root, status.as_bytes())
}

/// The paths in a `git status --porcelain=v1 -z` listing, as
/// [`list_working_tree_changes`] reports them.
fn working_tree_changes(repo_root: &Path, porcelain: &[u8]) -> Result<Vec<String>> {
    Ok(parse_porcelain_v1_z(porcelain)?
        .into_iter()
        .filter(|entry| !(entry.is_untracked() && is_tool_artifact(repo_root, &entry.path)))
        .map(|entry| entry.path)
        .collect())
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
/// * `Err` if git command fails or prints a listing that does not parse
pub fn get_uncommitted_changes_summary(repo_root: &Path) -> Result<String> {
    Ok(uncommitted_changes_summary(&status_entries(repo_root)?))
}

/// Read startup status once through the saved operator checkout identity.
pub(crate) fn pinned_uncommitted_changes(repo_root: &Path) -> Result<(bool, String)> {
    crate::git::worktree::require_main_identity(repo_root)?;
    let status = WorktreeGit::pinned_main(repo_root)?.run_checked(&STATUS_Z)?;
    let entries = parse_porcelain_v1_z(status.as_bytes())?;
    Ok((is_dirty(&entries), uncommitted_changes_summary(&entries)))
}

/// The status entries of the checkout at `repo_root`, through [`run_git`].
fn status_entries(repo_root: &Path) -> Result<Vec<StatusEntry>> {
    let output = run_git(&STATUS_Z, repo_root)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {stderr}");
    }
    parse_porcelain_v1_z(&output.stdout)
}

/// Whether some entry is staged or modified; untracked files do not count.
fn is_dirty(entries: &[StatusEntry]) -> bool {
    entries.iter().any(|entry| !entry.is_untracked())
}

fn uncommitted_changes_summary(entries: &[StatusEntry]) -> String {
    let changed = |column: u8| column != b' ' && column != b'?';
    let tracked = || entries.iter().filter(|entry| !entry.is_untracked());
    let staged: Vec<&str> = tracked()
        .filter(|entry| changed(entry.index()))
        .map(|entry| entry.path.as_str())
        .collect();
    let modified: Vec<&str> = tracked()
        .filter(|entry| changed(entry.worktree()))
        .map(|entry| entry.path.as_str())
        .collect();

    let mut summary = String::new();
    if !staged.is_empty() {
        summary.push_str(&format!("Staged: {}\n", staged.join(", ")));
    }
    if !modified.is_empty() {
        summary.push_str(&format!("Modified: {}\n", modified.join(", ")));
    }

    summary
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
