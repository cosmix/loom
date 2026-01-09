//! Git worktree management for parallel stage isolation
//!
//! Each parallel stage gets its own worktree to prevent file conflicts.
//! Worktrees are created in .worktrees/{stage_id}/ directories.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::worktree::Worktree;

/// Create a new worktree for a stage
///
/// Creates: .worktrees/{stage_id}/ with branch loom/{stage_id}
/// Also creates symlink .worktrees/{stage_id}/.work -> main .work/
pub fn create_worktree(stage_id: &str, repo_root: &Path) -> Result<Worktree> {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let branch_name = format!("loom/{stage_id}");

    // Ensure .worktrees directory exists
    let worktrees_dir = repo_root.join(".worktrees");
    if !worktrees_dir.exists() {
        std::fs::create_dir_all(&worktrees_dir)
            .with_context(|| "Failed to create .worktrees directory")?;
    }

    // Check if worktree already exists
    if worktree_path.exists() {
        bail!("Worktree already exists at {}", worktree_path.display());
    }

    // Create the worktree with a new branch
    // git worktree add .worktrees/{stage_id} -b loom/{stage_id}
    let output = Command::new("git")
        .args(["worktree", "add", "-b", &branch_name])
        .arg(&worktree_path)
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to execute git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // If branch already exists, try without -b
        if stderr.contains("already exists") {
            let output = Command::new("git")
                .args(["worktree", "add"])
                .arg(&worktree_path)
                .arg(&branch_name)
                .current_dir(repo_root)
                .output()
                .with_context(|| "Failed to execute git worktree add")?;

            if !output.status.success() {
                let stderr_msg = String::from_utf8_lossy(&output.stderr);
                bail!("git worktree add failed: {stderr_msg}");
            }
        } else {
            bail!("git worktree add failed: {stderr}");
        }
    }

    // Create symlink to main .work/ directory
    ensure_work_symlink(&worktree_path, repo_root)?;

    // Set up .claude/ directory for worktree
    // We create a real directory and symlink both CLAUDE.md and settings.local.json
    // from main repo. This ensures:
    // 1. Instructions (CLAUDE.md) are shared
    // 2. Permissions (settings.local.json) are shared - approvals propagate across sessions
    let main_claude_dir = repo_root.join(".claude");
    let worktree_claude_dir = worktree_path.join(".claude");

    if main_claude_dir.exists() && !worktree_claude_dir.exists() {
        // Create real .claude/ directory in worktree
        std::fs::create_dir_all(&worktree_claude_dir)
            .with_context(|| "Failed to create .claude directory in worktree")?;

        // Symlink CLAUDE.md from main repo for instruction inheritance
        let main_claude_md = main_claude_dir.join("CLAUDE.md");
        if main_claude_md.exists() {
            let worktree_claude_md = worktree_claude_dir.join("CLAUDE.md");
            let relative_claude_md = Path::new("../../../.claude/CLAUDE.md");

            #[cfg(unix)]
            std::os::unix::fs::symlink(relative_claude_md, &worktree_claude_md)
                .with_context(|| "Failed to create CLAUDE.md symlink in worktree")?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(relative_claude_md, &worktree_claude_md)
                .with_context(|| "Failed to create CLAUDE.md symlink in worktree")?;
        }

        // Create settings.local.json with trust and auto-accept settings merged with main repo settings
        // This ensures:
        // 1. hasTrustDialogAccepted: true - skips the "Yes, proceed / No, exit" prompt
        // 2. permissions.defaultMode: "acceptEdits" - auto-accepts file edits without prompting
        // 3. All permissions from main repo are inherited
        let main_settings = main_claude_dir.join("settings.local.json");
        let worktree_settings = worktree_claude_dir.join("settings.local.json");
        create_worktree_settings(&main_settings, &worktree_settings)?;
    }

    // Symlink project-root CLAUDE.md (distinct from .claude/CLAUDE.md)
    // This ensures Claude Code instances in worktrees have access to project instructions
    // without needing to read from the main repo outside the worktree
    let main_root_claude_md = repo_root.join("CLAUDE.md");
    let worktree_root_claude_md = worktree_path.join("CLAUDE.md");

    if main_root_claude_md.exists() && !worktree_root_claude_md.exists() {
        // Relative path from .worktrees/{stage_id}/CLAUDE.md to ../../CLAUDE.md
        let relative_root_claude_md = Path::new("../../CLAUDE.md");

        #[cfg(unix)]
        std::os::unix::fs::symlink(relative_root_claude_md, &worktree_root_claude_md)
            .with_context(|| "Failed to create root CLAUDE.md symlink in worktree")?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(relative_root_claude_md, &worktree_root_claude_md)
            .with_context(|| "Failed to create root CLAUDE.md symlink in worktree")?;
    }

    let mut worktree = Worktree::new(stage_id.to_string(), worktree_path, branch_name);
    worktree.mark_active();

    Ok(worktree)
}

/// Creates or restores the .work symlink in a worktree.
///
/// Used during worktree creation and merge failure recovery.
/// The symlink points from .worktrees/{stage_id}/.work to ../../.work (the main repo's .work/).
pub fn ensure_work_symlink(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    let main_work_dir = repo_root.join(".work");
    let worktree_work_link = worktree_path.join(".work");
    let relative_work_path = Path::new("../../.work");

    if main_work_dir.exists() && !worktree_work_link.exists() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(relative_work_path, &worktree_work_link)
            .with_context(|| "Failed to create .work symlink in worktree")?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(relative_work_path, &worktree_work_link)
            .with_context(|| "Failed to create .work symlink in worktree")?;
    }
    Ok(())
}

/// Create settings.local.json for a worktree with trust and auto-accept settings.
///
/// This function:
/// 1. Reads the main repo's settings.local.json (if it exists)
/// 2. Sets `hasTrustDialogAccepted: true` to skip the trust prompt
/// 3. Sets `permissions.defaultMode: "acceptEdits"` to auto-accept file edits
/// 4. Writes the merged result to the worktree
///
/// This solves two issues:
/// - Issue 9: Eliminates the "Yes, proceed / No, exit" prompt on session start
/// - Issue 10: Enables auto-accept edits for seamless operation
fn create_worktree_settings(main_settings: &Path, worktree_settings: &Path) -> Result<()> {
    // Start with main repo settings or empty object
    let mut settings: Value = if main_settings.exists() {
        let content = std::fs::read_to_string(main_settings)
            .with_context(|| "Failed to read main repo settings.local.json")?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Ensure settings is an object
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.local.json must be a JSON object"))?;

    // Set hasTrustDialogAccepted to skip the trust prompt
    obj.insert("hasTrustDialogAccepted".to_string(), json!(true));

    // Ensure permissions object exists and set defaultMode to acceptEdits
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    permissions.insert("defaultMode".to_string(), json!("acceptEdits"));

    // Write the merged settings
    let content = serde_json::to_string_pretty(&settings)
        .with_context(|| "Failed to serialize settings")?;
    std::fs::write(worktree_settings, content)
        .with_context(|| "Failed to write worktree settings.local.json")?;

    Ok(())
}

/// Remove a worktree
///
/// Runs: git worktree remove .worktrees/{stage_id}
pub fn remove_worktree(stage_id: &str, repo_root: &Path, force: bool) -> Result<()> {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);

    if !worktree_path.exists() {
        bail!("Worktree does not exist: {}", worktree_path.display());
    }

    // Remove the .work symlink first to avoid issues
    let work_link = worktree_path.join(".work");
    if work_link.exists() || work_link.is_symlink() {
        std::fs::remove_file(&work_link).ok(); // Ignore errors
    }

    // Remove the .claude directory (it's a real directory now, not a symlink)
    let claude_dir = worktree_path.join(".claude");
    if claude_dir.exists() {
        std::fs::remove_dir_all(&claude_dir).ok(); // Ignore errors
    } else if claude_dir.is_symlink() {
        // Handle legacy symlink case
        std::fs::remove_file(&claude_dir).ok();
    }

    // Remove the root CLAUDE.md symlink
    let root_claude_md = worktree_path.join("CLAUDE.md");
    if root_claude_md.exists() || root_claude_md.is_symlink() {
        std::fs::remove_file(&root_claude_md).ok(); // Ignore errors
    }

    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }

    let output = Command::new("git")
        .args(&args)
        .arg(&worktree_path)
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to execute git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree remove failed: {stderr}");
    }

    Ok(())
}

/// List all worktrees
pub fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeInfo>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to execute git worktree list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree list failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_worktree_list(&stdout)
}

/// Parsed worktree information
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub is_bare: bool,
}

/// Parse git worktree list --porcelain output
fn parse_worktree_list(output: &str) -> Result<Vec<WorktreeInfo>> {
    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeInfo> = None;

    for line in output.lines() {
        if line.starts_with("worktree ") {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            let path = line.strip_prefix("worktree ").unwrap_or("");
            current = Some(WorktreeInfo {
                path: PathBuf::from(path),
                head: String::new(),
                branch: None,
                is_bare: false,
            });
        } else if line.starts_with("HEAD ") {
            if let Some(ref mut wt) = current {
                wt.head = line.strip_prefix("HEAD ").unwrap_or("").to_string();
            }
        } else if line.starts_with("branch ") {
            if let Some(ref mut wt) = current {
                let branch_line = line.strip_prefix("branch ").unwrap_or("");
                let branch_name = branch_line
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch_line);
                wt.branch = Some(branch_name.to_string());
            }
        } else if line == "bare" {
            if let Some(ref mut wt) = current {
                wt.is_bare = true;
            }
        }
    }

    if let Some(wt) = current {
        worktrees.push(wt);
    }

    Ok(worktrees)
}

/// Clean orphaned worktrees (prune)
pub fn clean_worktrees(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to execute git worktree prune")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree prune failed: {stderr}");
    }

    Ok(())
}

/// Check if a worktree exists for a stage
pub fn worktree_exists(stage_id: &str, repo_root: &Path) -> bool {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    worktree_path.exists()
}

/// Get an existing worktree or create a new one
///
/// If a valid worktree exists at .worktrees/{stage_id}/, reuses it.
/// If the directory exists but is not a valid worktree, removes it and recreates.
/// Otherwise, creates a new worktree.
///
/// This function is idempotent and safe to call multiple times for the same stage.
pub fn get_or_create_worktree(stage_id: &str, repo_root: &Path) -> Result<Worktree> {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let branch_name = format!("loom/{stage_id}");

    if worktree_path.exists() {
        // Check if it's a valid git worktree by looking for the .git file
        // Git worktrees have a .git file (not directory) that points to the main repo
        let git_file = worktree_path.join(".git");
        if git_file.exists() {
            // Verify it's actually tracked by git worktree list
            if is_valid_git_worktree(&worktree_path, repo_root)? {
                // Valid worktree exists, return it
                let mut worktree = Worktree::new(stage_id.to_string(), worktree_path, branch_name);
                worktree.mark_active();
                return Ok(worktree);
            }
        }

        // Directory exists but is not a valid worktree - remove it
        // First try to prune any stale worktree references
        let _ = clean_worktrees(repo_root);

        // Now remove the directory
        std::fs::remove_dir_all(&worktree_path).with_context(|| {
            format!(
                "Failed to remove invalid worktree directory: {}",
                worktree_path.display()
            )
        })?;
    }

    // Create new worktree
    create_worktree(stage_id, repo_root)
}

/// Check if a path is a valid git worktree tracked by the repository
fn is_valid_git_worktree(worktree_path: &Path, repo_root: &Path) -> Result<bool> {
    let worktrees = list_worktrees(repo_root)?;

    // Canonicalize paths for comparison to handle symlinks and relative paths
    let worktree_canonical = worktree_path.canonicalize().ok();

    for wt in worktrees {
        let wt_canonical = wt.path.canonicalize().ok();

        // Compare canonical paths if available, otherwise compare as-is
        let paths_match = match (&worktree_canonical, &wt_canonical) {
            (Some(a), Some(b)) => a == b,
            _ => wt.path == worktree_path,
        };

        if paths_match {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Get the path to a worktree
pub fn get_worktree_path(stage_id: &str, repo_root: &Path) -> PathBuf {
    repo_root.join(".worktrees").join(stage_id)
}

/// Check if git is available
pub fn check_git_available() -> Result<()> {
    let output = Command::new("git")
        .args(["--version"])
        .output()
        .with_context(|| "Git is not installed or not in PATH")?;

    if !output.status.success() {
        bail!("Git is not working properly");
    }

    Ok(())
}

/// Check if git worktree is supported (git 2.15+)
pub fn check_worktree_support() -> Result<()> {
    check_git_available()?;

    let output = Command::new("git").args(["worktree", "list"]).output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => bail!("Git worktree feature not supported. Requires git 2.15+"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list() {
        let output = r#"worktree /home/user/repo
HEAD abc123def456
branch main

worktree /home/user/repo/.worktrees/stage-1
HEAD def789abc012
branch loom/stage-1
"#;

        let worktrees = parse_worktree_list(output).unwrap();
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].branch, Some("main".to_string()));
        assert_eq!(worktrees[1].branch, Some("loom/stage-1".to_string()));
    }

    #[test]
    fn test_get_worktree_path() {
        let repo_root = Path::new("/home/user/repo");
        let path = get_worktree_path("stage-1", repo_root);
        assert_eq!(path, PathBuf::from("/home/user/repo/.worktrees/stage-1"));
    }
}
