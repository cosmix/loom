//! Core branch operations: create, delete, list, check existence

use anyhow::Result;
use std::path::Path;

use super::info::{list_refs, BranchInfo};
use crate::git::runner::{run_git, run_git_bool, run_git_checked};

/// Create a new branch from a base
pub fn create_branch(name: &str, base: Option<&str>, repo_root: &Path) -> Result<()> {
    let mut args = vec!["branch", name];
    if let Some(b) = base {
        args.push(b);
    }

    run_git_checked(&args, repo_root)?;
    Ok(())
}

/// Delete a branch
pub fn delete_branch(name: &str, force: bool, repo_root: &Path) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };

    run_git_checked(&["branch", flag, name], repo_root)?;
    Ok(())
}

/// Get the current branch name, or `HEAD` on a detached HEAD.
///
/// Reads the symbolic ref rather than `rev-parse --abbrev-ref`, which prints
/// `heads/<name>` when a tag of the same name exists.
pub fn current_branch(repo_root: &Path) -> Result<String> {
    current_branch_with(&crate::git::worktree::WorktreeGit::discovered(repo_root))
}

pub(crate) fn current_branch_with(git: &crate::git::worktree::WorktreeGit) -> Result<String> {
    let output = git.run(&["symbolic-ref", "-q", "HEAD"])?;
    if output.status.success() {
        let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(head
            .strip_prefix("refs/heads/")
            .unwrap_or(&head)
            .to_string());
    }
    // `-q` exits 1 with no output on a detached HEAD; anything else is a failure.
    if output.status.code() == Some(1) {
        return Ok("HEAD".to_string());
    }
    anyhow::bail!(
        "git symbolic-ref HEAD failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

pub(crate) fn resolve_target_branch_with(
    config_branch: &Option<String>,
    git: &crate::git::worktree::WorktreeGit,
) -> Result<String> {
    if let Some(branch) = config_branch {
        return Ok(branch.clone());
    }
    let output = git.run(&["symbolic-ref", "refs/remotes/origin/HEAD"])?;
    if output.status.success() {
        let value = String::from_utf8_lossy(&output.stdout);
        if let Some(branch) = value.trim().strip_prefix("refs/remotes/origin/") {
            return Ok(branch.to_string());
        }
    }
    for branch in ["main", "master"] {
        if git
            .run(&["rev-parse", "--verify", &format!("refs/heads/{branch}")])?
            .status
            .success()
        {
            return Ok(branch.to_string());
        }
    }
    Ok("main".into())
}

/// List all branches
pub fn list_branches(repo_root: &Path) -> Result<Vec<BranchInfo>> {
    list_refs(repo_root, None)
}

/// Check if a branch exists
pub fn branch_exists(name: &str, repo_root: &Path) -> Result<bool> {
    let ref_path = format!("refs/heads/{name}");
    Ok(run_git_bool(
        &["rev-parse", "--verify", &ref_path],
        repo_root,
    ))
}

/// Get the default branch (main or master)
pub fn default_branch(repo_root: &Path) -> Result<String> {
    // Try to get from remote origin
    if let Ok(output) = run_git(&["symbolic-ref", "refs/remotes/origin/HEAD"], repo_root) {
        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout);
            // refs/remotes/origin/main -> main
            if let Some(branch) = result.trim().strip_prefix("refs/remotes/origin/") {
                return Ok(branch.to_string());
            }
        }
    }

    // Fall back to checking if main or master exists
    if branch_exists("main", repo_root)? {
        return Ok("main".to_string());
    }
    if branch_exists("master", repo_root)? {
        return Ok("master".to_string());
    }

    anyhow::bail!("Could not determine default branch")
}

/// List loom branches (branches starting with loom/)
pub fn list_loom_branches(repo_root: &Path) -> Result<Vec<String>> {
    Ok(list_refs(repo_root, None)?
        .into_iter()
        .map(|branch| branch.name)
        .filter(|name| name.starts_with("loom/"))
        .collect())
}

/// Resolve target branch from config value or repo default
///
/// Uses the provided config branch if available, otherwise falls back to
/// the repository's default branch (main/master).
pub fn resolve_target_branch(config_branch: &Option<String>, repo_root: &Path) -> String {
    config_branch
        .clone()
        .unwrap_or_else(|| default_branch(repo_root).unwrap_or_else(|_| "main".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        // Create initial commit on main
        std::fs::write(repo_root.join("README.md"), "# Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        // Rename to main if needed
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        temp_dir
    }

    #[test]
    fn test_resolve_target_branch_with_config() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        let config_branch = Some("custom-branch".to_string());
        let result = resolve_target_branch(&config_branch, repo_root);

        assert_eq!(result, "custom-branch");
    }

    #[test]
    fn test_resolve_target_branch_without_config() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        let config_branch = None;
        let result = resolve_target_branch(&config_branch, repo_root);

        assert_eq!(result, "main");
    }

    fn git_in(repo_root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
    }

    #[test]
    fn current_branch_ignores_a_tag_named_like_the_branch() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();
        assert_eq!(current_branch(repo_root).unwrap(), "main");

        git_in(repo_root, &["tag", "main"]);
        assert_eq!(current_branch(repo_root).unwrap(), "main");
    }

    #[test]
    fn current_branch_is_head_when_detached() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();
        git_in(repo_root, &["checkout", "-q", "--detach"]);
        assert_eq!(current_branch(repo_root).unwrap(), "HEAD");
    }

    /// Fields come out exact whatever the display would pad or mark: a current
    /// branch, a branch checked out in a linked worktree, names of very
    /// different widths, distinct hashes and an empty subject.
    #[test]
    fn listed_fields_are_exact_for_current_linked_and_padded_branches() {
        let temp_dir = init_test_repo();
        let repo = temp_dir.path();
        let main_hash = run_git_checked(&["rev-parse", "HEAD"], repo).unwrap();
        git_in(repo, &["branch", "x"]);
        let long = "loom/a-branch-name-much-wider-than-any-other";
        git_in(repo, &["branch", long]);
        let linked = repo.join("linked");
        git_in(repo, &["worktree", "add", linked.to_str().unwrap(), long]);
        git_in(
            &linked,
            &["commit", "--allow-empty", "--allow-empty-message", "-m", ""],
        );
        let linked_hash = run_git_checked(&["rev-parse", "HEAD"], &linked).unwrap();
        assert_ne!(main_hash, linked_hash);

        let branches = list_branches(repo).unwrap();

        let field = |name: &str| branches.iter().find(|b| b.name == name).unwrap();
        assert_eq!(branches.len(), 3, "{branches:?}");
        assert!(field("main").is_current);
        assert_eq!(field("main").commit_hash, main_hash.trim());
        assert_eq!(field("main").commit_message, "Initial commit");
        assert!(!field("x").is_current);
        assert_eq!(field("x").commit_hash, main_hash.trim());
        assert!(
            !field(long).is_current,
            "a linked checkout is not current here"
        );
        assert_eq!(field(long).commit_hash, linked_hash.trim());
        assert_eq!(field(long).commit_message, "");
        assert!(!branches.iter().any(|b| b.name.starts_with(['+', '*', ' '])));
    }

    #[test]
    fn listing_and_merged_checks_include_linked_worktree_refs() {
        let temp_dir = init_test_repo();
        let repo = temp_dir.path();
        git_in(
            repo,
            &["commit", "--allow-empty", "--allow-empty-message", "-m", ""],
        );
        let hash = run_git_checked(&["rev-parse", "HEAD"], repo).unwrap();
        git_in(repo, &["branch", "loom/a-long-linked-name"]);
        git_in(repo, &["branch", "loom/z"]);
        let linked = repo.join("linked");
        git_in(
            repo,
            &[
                "worktree",
                "add",
                linked.to_str().unwrap(),
                "loom/a-long-linked-name",
            ],
        );
        let branches = list_branches(repo).unwrap();
        for name in ["main", "loom/a-long-linked-name", "loom/z"] {
            let branch = branches.iter().find(|b| b.name == name).unwrap();
            assert_eq!(branch.commit_hash, hash);
            assert_eq!(branch.commit_message, "");
            assert_eq!(branch.is_current, name == "main");
        }
        assert_eq!(
            list_loom_branches(repo).unwrap(),
            vec!["loom/a-long-linked-name", "loom/z"]
        );
        assert!(super::super::is_branch_merged("loom/a-long-linked-name", "main", repo).unwrap());
        assert!(!super::super::is_branch_merged("loom/a-long", "main", repo).unwrap());
        assert!(super::super::is_branch_merged("loom/z", "missing", repo).is_err());
        assert_eq!(
            super::super::cleanup_merged_branches("main", repo).unwrap(),
            vec!["loom/z"]
        );
    }
}
