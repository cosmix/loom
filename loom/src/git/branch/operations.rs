//! Core branch operations: create, delete, list, check existence

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use super::info::{parse_branch_list, BranchInfo};

/// Create a new branch from a base
pub fn create_branch(name: &str, base: Option<&str>, repo_root: &Path) -> Result<()> {
    let mut args = vec!["branch", name];
    if let Some(b) = base {
        args.push(b);
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to create branch {name}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch failed: {stderr}");
    }

    Ok(())
}

/// Delete a branch
pub fn delete_branch(name: &str, force: bool, repo_root: &Path) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };

    let output = Command::new("git")
        .args(["branch", flag, name])
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to delete branch {name}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch delete failed: {stderr}");
    }

    Ok(())
}

/// Get the current branch name
pub fn current_branch(repo_root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to get current branch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git rev-parse failed: {stderr}");
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(branch)
}

/// List all branches
pub fn list_branches(repo_root: &Path) -> Result<Vec<BranchInfo>> {
    let output = Command::new("git")
        .args(["branch", "-v", "--no-color"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to list branches")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch -v failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_branch_list(&stdout)
}

/// Check if a branch exists
pub fn branch_exists(name: &str, repo_root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{name}")])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to check branch existence")?;

    Ok(output.status.success())
}

/// Get the default branch (main or master)
pub fn default_branch(repo_root: &Path) -> Result<String> {
    // Try to get from remote origin
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(repo_root)
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let result = String::from_utf8_lossy(&out.stdout);
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

    bail!("Could not determine default branch")
}

/// List loom branches (branches starting with loom/)
pub fn list_loom_branches(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["branch", "--list", "loom/*"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to list loom branches")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch --list failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches: Vec<String> = stdout
        .lines()
        .map(|s| s.trim().trim_start_matches('*').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(branches)
}
