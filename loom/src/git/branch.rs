//! Git branch management operations

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

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

/// Branch information
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub commit_hash: String,
    pub commit_message: String,
}

/// Parse git branch -v output
fn parse_branch_list(output: &str) -> Result<Vec<BranchInfo>> {
    let mut branches = Vec::new();

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        let is_current = line.starts_with('*');
        let line = line.trim_start_matches('*').trim();

        // Parse: branch_name commit_hash commit_message
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() >= 2 {
            let name = parts[0].trim().to_string();
            let commit_hash = parts[1].trim().to_string();
            let commit_message = if parts.len() > 2 {
                parts[2].trim().to_string()
            } else {
                String::new()
            };

            branches.push(BranchInfo {
                name,
                is_current,
                commit_hash,
                commit_message,
            });
        }
    }

    Ok(branches)
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

/// Clean up loom branches that have been merged
pub fn cleanup_merged_branches(target_branch: &str, repo_root: &Path) -> Result<Vec<String>> {
    // Get merged branches
    let output = Command::new("git")
        .args(["branch", "--merged", target_branch])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to get merged branches")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let merged_loom_branches: Vec<String> = stdout
        .lines()
        .map(|s| s.trim().trim_start_matches('*').trim().to_string())
        .filter(|s| s.starts_with("loom/"))
        .collect();

    let mut deleted = Vec::new();
    for branch in merged_loom_branches {
        if delete_branch(&branch, false, repo_root).is_ok() {
            deleted.push(branch);
        }
    }

    Ok(deleted)
}

/// Get the stage ID from a loom branch name
pub fn stage_id_from_branch(branch_name: &str) -> Option<String> {
    branch_name.strip_prefix("loom/").map(String::from)
}

/// Generate loom branch name from stage ID
pub fn branch_name_for_stage(stage_id: &str) -> String {
    format!("loom/{stage_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_branch_list() {
        let output = r#"* main       abc1234 Initial commit
  loom/stage-1 def5678 Add feature
  feature    789abcd Work in progress
"#;

        let branches = parse_branch_list(output).unwrap();
        assert_eq!(branches.len(), 3);

        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_current);

        assert_eq!(branches[1].name, "loom/stage-1");
        assert!(!branches[1].is_current);
    }

    #[test]
    fn test_stage_id_from_branch() {
        assert_eq!(
            stage_id_from_branch("loom/stage-1"),
            Some("stage-1".to_string())
        );
        assert_eq!(stage_id_from_branch("main"), None);
    }

    #[test]
    fn test_branch_name_for_stage() {
        assert_eq!(branch_name_for_stage("stage-1"), "loom/stage-1");
        assert_eq!(branch_name_for_stage("my-feature"), "loom/my-feature");
    }
}
