//! Branch ancestry and merge status checking

use anyhow::Result;
use std::path::Path;

use crate::git::runner::{run_git, run_git_checked};

#[cfg(test)]
use std::process::Command;

/// Check if a commit is an ancestor of a branch.
///
/// Uses `git merge-base --is-ancestor` to check if the given commit
/// is reachable from the branch head.
///
/// # Arguments
/// * `commit_sha` - The commit SHA to check
/// * `branch` - The branch name to check against
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// * `Ok(true)` if the commit is an ancestor of (or equal to) the branch head
/// * `Ok(false)` if the commit is not an ancestor
/// * `Err` if the git command fails (e.g., invalid commit/branch)
pub fn is_ancestor_of(commit_sha: &str, branch: &str, repo_root: &Path) -> Result<bool> {
    let output = run_git(
        &["merge-base", "--is-ancestor", commit_sha, branch],
        repo_root,
    )?;
    Ok(output.status.success())
}

/// Get the HEAD commit SHA of a branch
///
/// Uses `git rev-parse` to resolve the branch name to its HEAD commit SHA.
///
/// # Arguments
/// * `branch` - The branch name to get HEAD for (e.g., "loom/stage-1")
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// * `Ok(sha)` - The full commit SHA of the branch HEAD
/// * `Err` if the branch doesn't exist or git command fails
pub fn get_branch_head(branch: &str, repo_root: &Path) -> Result<String> {
    run_git_checked(&["rev-parse", branch], repo_root)
}

/// Count commits on `branch` that are not on `base`.
///
/// Uses `git rev-list --count <base>..<branch>`. Returns 0 if the branch
/// is fully merged into base, or if either ref is missing (treated as "no
/// new commits" — callers that need to distinguish "no commits" from
/// "branch missing" should check `branch_exists` first).
///
/// # Arguments
/// * `branch` - The branch with potentially new commits (e.g., `loom/<stage>`)
/// * `base` - The branch to compare against (e.g., `main`)
/// * `repo_root` - Path to the git repository root
pub fn commits_ahead_of(branch: &str, base: &str, repo_root: &Path) -> Result<usize> {
    let range = format!("{base}..{branch}");
    let output = run_git(&["rev-list", "--count", &range], repo_root)?;
    if !output.status.success() {
        // Missing branch / unknown ref → treat as zero so callers can
        // proceed defensively. Real git errors (e.g., not a repo) still
        // surface as Err via run_git.
        return Ok(0);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().parse::<usize>().unwrap_or(0))
}

/// Check if a branch has been fully merged into a target branch
///
/// This is used to detect manual merges performed outside loom.
/// If a loom branch has been merged into the target branch (e.g., main),
/// we can detect this and trigger cleanup.
///
/// # Arguments
/// * `branch` - The branch to check (e.g., "loom/stage-1")
/// * `target` - The target branch to check against (e.g., "main")
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// * `Ok(true)` if the branch has been merged into target
/// * `Ok(false)` if the branch has not been merged
/// * `Err` if git command fails
pub fn is_branch_merged(branch: &str, target: &str, repo_root: &Path) -> Result<bool> {
    let stdout = run_git_checked(&["branch", "--merged", target, "--list", branch], repo_root)?;
    Ok(stdout
        .lines()
        .any(|line| line.trim().trim_start_matches('*').trim() == branch))
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
    fn test_is_ancestor_of() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        // Get first commit SHA
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        let first_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Create second commit
        std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
        Command::new("git")
            .args(["add", "file2.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Get second commit SHA (current HEAD)
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        let second_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Get current branch name
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        let branch_name = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Test: first commit should be an ancestor of current branch
        assert!(is_ancestor_of(&first_commit, &branch_name, repo_path).unwrap());

        // Test: second commit (HEAD) should be an ancestor of itself
        assert!(is_ancestor_of(&second_commit, &branch_name, repo_path).unwrap());

        // Test: second commit should NOT be an ancestor of first commit
        assert!(!is_ancestor_of(&second_commit, &first_commit, repo_path).unwrap());
    }

    #[test]
    fn test_get_branch_head() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        // Get branch name
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        let branch_name = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Get expected HEAD
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        let expected_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Test get_branch_head returns correct SHA
        let result = get_branch_head(&branch_name, repo_path).unwrap();
        assert_eq!(result, expected_sha);

        // Test get_branch_head fails for non-existent branch
        let result = get_branch_head("nonexistent-branch", repo_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_commits_ahead_of() {
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        let base = String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(repo_path)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();

        // No divergence yet: branch == base, expect 0.
        assert_eq!(commits_ahead_of(&base, &base, repo_path).unwrap(), 0);

        // Create a sibling branch with one new commit.
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        std::fs::write(repo_path.join("new.txt"), "x").unwrap();
        Command::new("git")
            .args(["add", "new.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "feature commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // feature has 1 commit ahead of base.
        assert_eq!(commits_ahead_of("feature", &base, repo_path).unwrap(), 1);
        // base has 0 commits ahead of feature (base is the parent).
        assert_eq!(commits_ahead_of(&base, "feature", repo_path).unwrap(), 0);

        // Missing branch → defensive 0, not an error.
        assert_eq!(
            commits_ahead_of("nonexistent", &base, repo_path).unwrap(),
            0
        );
    }
}
