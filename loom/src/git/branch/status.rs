//! Git status checking for uncommitted changes

use anyhow::{bail, Context, Result};
use std::path::Path;
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
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to check git status")?;

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
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to check git status")?;

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
