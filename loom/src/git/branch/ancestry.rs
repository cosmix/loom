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
    // `git merge-base --is-ancestor` exit codes:
    //   0   -> commit IS an ancestor of branch
    //   1   -> commit is NOT an ancestor
    //   else (commonly 128) -> a real git error (bad/missing ref, not a repo, …)
    // The previous `Ok(status.success())` collapsed 1 and 128 into `Ok(false)`,
    // making the documented `Err` contract unreachable. Callers that fall back to
    // metadata on `Err` (check_merge_state, repair) must be able to tell a genuine
    // "not an ancestor" from "git couldn't answer" (e.g. a gc'd/rewritten commit).
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "git merge-base --is-ancestor failed (exit {}):\n\
                 Directory: {}\n\
                 Commit: {commit_sha}\n\
                 Branch: {branch}\n\
                 Stderr: {}",
                output
                    .status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string()),
                repo_root.display(),
                if stderr.trim().is_empty() {
                    "(empty)"
                } else {
                    stderr.trim()
                }
            )
        }
    }
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
        // A non-zero exit is ambiguous: it can mean either a missing ref
        // (callers want a defensive 0) or a genuine git error (callers that
        // gate destructive deletion on commits-ahead must NOT see a false 0).
        // Disambiguate by probing whether `branch` actually resolves. If the
        // branch is genuinely absent there are no commits to lose, so 0 is
        // safe; otherwise surface the error so deletion callers fail closed.
        if !branch_exists_ref(branch, repo_root) {
            return Ok(0);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "git rev-list --count {range} failed (exit {}):\n\
             Directory: {}\n\
             Stderr: {}",
            output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string()),
            repo_root.display(),
            if stderr.trim().is_empty() {
                "(empty)"
            } else {
                stderr.trim()
            }
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().parse::<usize>().unwrap_or(0))
}

/// Resolve whether a ref (branch name or SHA) exists, swallowing all errors.
///
/// Used by [`commits_ahead_of`] to disambiguate "range failed because the
/// branch is missing" (safe → 0) from "range failed for another reason"
/// (must surface as `Err`). Kept local and infallible on purpose: the caller
/// is already in an error path and only needs a best-effort yes/no.
fn branch_exists_ref(reference: &str, repo_root: &Path) -> bool {
    run_git(&["rev-parse", "--verify", "--quiet", reference], repo_root)
        .map(|o| o.status.success())
        .unwrap_or(false)
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
    fn test_is_ancestor_of_errors_on_bad_ref() {
        // A nonexistent commit/branch makes git exit 128, which must surface
        // as Err (not a silent Ok(false)) so callers can fall back to metadata.
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        let result = is_ancestor_of(
            "0000000000000000000000000000000000000000",
            "main",
            repo_path,
        );
        assert!(
            result.is_err(),
            "git error (exit 128) must surface as Err, got {result:?}"
        );
    }

    #[test]
    fn test_commits_ahead_of_errors_on_bad_base() {
        // Branch exists but base ref is bogus → git error, must be Err (not 0).
        let temp_dir = init_test_repo();
        let repo_path = temp_dir.path();

        let branch = String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(repo_path)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();

        let result = commits_ahead_of(&branch, "nonexistent-base", repo_path);
        assert!(
            result.is_err(),
            "real git error with an existing branch must surface as Err, got {result:?}"
        );
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
