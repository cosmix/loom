//! Core branch operations: create, delete, list, check existence

use anyhow::Result;
use std::path::Path;

use crate::git::runner::{run_git, run_git_bool, run_git_checked};
use super::info::{parse_branch_list, BranchInfo};

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

/// Get the current branch name
pub fn current_branch(repo_root: &Path) -> Result<String> {
    run_git_checked(&["rev-parse", "--abbrev-ref", "HEAD"], repo_root)
}

/// List all branches
pub fn list_branches(repo_root: &Path) -> Result<Vec<BranchInfo>> {
    let stdout = run_git_checked(&["branch", "-v", "--no-color"], repo_root)?;
    parse_branch_list(&stdout)
}

/// Check if a branch exists
pub fn branch_exists(name: &str, repo_root: &Path) -> Result<bool> {
    let ref_path = format!("refs/heads/{name}");
    Ok(run_git_bool(&["rev-parse", "--verify", &ref_path], repo_root))
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
    let stdout = run_git_checked(&["branch", "--list", "loom/*"], repo_root)?;
    let branches: Vec<String> = stdout
        .lines()
        .map(|s| s.trim().trim_start_matches('*').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(branches)
}
