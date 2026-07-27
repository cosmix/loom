//! Git status checking for uncommitted changes

use anyhow::{bail, Result};
use std::path::Path;

use crate::git::runner::run_git;

#[cfg(test)]
use std::process::Command;

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
/// `.git/info/exclude` are excluded by git itself.
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

    Ok(stdout
        .lines()
        .filter(|line| line.len() > 3)
        // Porcelain v1: "XY path" — or "XY old -> new" for renames/copies.
        .map(|line| match line[3..].split_once(" -> ") {
            Some((_, destination)) => destination.to_string(),
            None => line[3..].to_string(),
        })
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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("file1.txt"), "content1").unwrap();
        Command::new("git")
            .args(["add", "file1.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        temp_dir
    }

    #[test]
    fn test_has_uncommitted_changes_clean_repo() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        assert!(!has_uncommitted_changes(repo_path).unwrap());
    }

    #[test]
    fn test_has_uncommitted_changes_staged_file() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
        Command::new("git")
            .args(["add", "file2.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        assert!(has_uncommitted_changes(repo_path).unwrap());
    }

    #[test]
    fn test_has_uncommitted_changes_modified_file() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

        assert!(has_uncommitted_changes(repo_path).unwrap());
    }

    #[test]
    fn test_has_uncommitted_changes_untracked_only() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("untracked.txt"), "untracked content").unwrap();

        // Untracked files should NOT be considered uncommitted changes
        assert!(!has_uncommitted_changes(repo_path).unwrap());
    }

    #[test]
    fn test_list_working_tree_changes_clean_repo() {
        let temp_dir = init_test_repo();

        assert!(list_working_tree_changes(temp_dir.path())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_list_working_tree_changes_includes_untracked() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("new_module.rs"), "fn feature() {}").unwrap();

        // An agent's brand-new file is untracked but is real work — unlike
        // has_uncommitted_changes, this must see it.
        assert_eq!(
            list_working_tree_changes(repo_path).unwrap(),
            vec!["new_module.rs".to_string()]
        );
        assert!(!has_uncommitted_changes(repo_path).unwrap());
    }

    #[test]
    fn test_list_working_tree_changes_includes_modified() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

        assert_eq!(
            list_working_tree_changes(repo_path).unwrap(),
            vec!["file1.txt".to_string()]
        );
    }

    #[test]
    fn test_list_working_tree_changes_reports_rename_destination() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["mv", "file1.txt", "renamed.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        assert_eq!(
            list_working_tree_changes(repo_path).unwrap(),
            vec!["renamed.txt".to_string()]
        );
    }

    #[test]
    fn test_list_working_tree_changes_omits_ignored_files() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join(".gitignore"), "generated.txt\n").unwrap();
        Command::new("git")
            .args(["add", ".gitignore"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Add gitignore"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        std::fs::write(repo_path.join("generated.txt"), "build artifact").unwrap();

        assert!(list_working_tree_changes(repo_path).unwrap().is_empty());
    }

    #[test]
    fn test_get_uncommitted_changes_summary_clean_repo() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        let summary = get_uncommitted_changes_summary(repo_path).unwrap();
        assert!(summary.is_empty());
    }

    #[test]
    fn test_get_uncommitted_changes_summary_staged_file() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
        Command::new("git")
            .args(["add", "file2.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let summary = get_uncommitted_changes_summary(repo_path).unwrap();
        assert!(summary.contains("Staged:"));
        assert!(summary.contains("file2.txt"));
    }

    #[test]
    fn test_get_uncommitted_changes_summary_modified_file() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

        let summary = get_uncommitted_changes_summary(repo_path).unwrap();
        assert!(summary.contains("Modified:"));
        assert!(summary.contains("file1.txt"));
    }

    #[test]
    fn test_get_uncommitted_changes_summary_both_staged_and_modified() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
        Command::new("git")
            .args(["add", "file2.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

        let summary = get_uncommitted_changes_summary(repo_path).unwrap();
        assert!(summary.contains("Staged:"));
        assert!(summary.contains("file2.txt"));
        assert!(summary.contains("Modified:"));
        assert!(summary.contains("file1.txt"));
    }

    #[test]
    fn test_get_uncommitted_changes_summary_untracked_only() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        std::fs::write(repo_path.join("untracked.txt"), "untracked content").unwrap();

        let summary = get_uncommitted_changes_summary(repo_path).unwrap();
        assert!(summary.is_empty());
    }
}
