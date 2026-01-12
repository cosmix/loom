//! Worktree operations
//!
//! Core CRUD operations for git worktrees: create, remove, list, get_or_create.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::worktree::Worktree;

use super::checks::is_valid_git_worktree;
use super::parser::{parse_worktree_list, WorktreeInfo};
use super::settings::{
    cleanup_worktree_settings, ensure_work_symlink, setup_claude_directory, setup_root_claude_md,
};

/// Create a new worktree for a stage
///
/// Creates: .worktrees/{stage_id}/ with branch loom/{stage_id}
/// Also creates symlink .worktrees/{stage_id}/.work -> main .work/
///
/// If `base_branch` is Some(branch), the new branch is created from that branch:
///   git worktree add -b loom/{stage_id} .worktrees/{stage_id} {branch}
/// If `base_branch` is None, the new branch is created from HEAD (current behavior).
pub fn create_worktree(
    stage_id: &str,
    repo_root: &Path,
    base_branch: Option<&str>,
) -> Result<Worktree> {
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
    // If base_branch is Some: git worktree add -b loom/{stage_id} .worktrees/{stage_id} {base_branch}
    // If base_branch is None: git worktree add -b loom/{stage_id} .worktrees/{stage_id} (from HEAD)
    let mut args = vec!["worktree", "add", "-b", &branch_name];
    let worktree_path_str = worktree_path.to_string_lossy();
    args.push(&worktree_path_str);
    if let Some(base) = base_branch {
        args.push(base);
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to execute git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // If branch already exists, delete it and recreate from correct base
        // This ensures we always use the correct base branch, not a stale one
        if stderr.contains("already exists") {
            // Delete the existing branch
            let delete_output = Command::new("git")
                .args(["branch", "-D", &branch_name])
                .current_dir(repo_root)
                .output()
                .with_context(|| format!("Failed to delete existing branch {branch_name}"))?;

            if !delete_output.status.success() {
                let delete_stderr = String::from_utf8_lossy(&delete_output.stderr);
                bail!("Failed to delete existing branch {branch_name}: {delete_stderr}");
            }

            // Retry creating the worktree with the correct base
            let retry_output = Command::new("git")
                .args(&args)
                .current_dir(repo_root)
                .output()
                .with_context(|| "Failed to execute git worktree add after branch deletion")?;

            if !retry_output.status.success() {
                let retry_stderr = String::from_utf8_lossy(&retry_output.stderr);
                bail!("git worktree add failed after branch deletion: {retry_stderr}");
            }
        } else {
            bail!("git worktree add failed: {stderr}");
        }
    }

    // Create symlink to main .work/ directory
    ensure_work_symlink(&worktree_path, repo_root)?;

    // Set up .claude/ directory for worktree
    setup_claude_directory(&worktree_path, repo_root)?;

    // Symlink project-root CLAUDE.md
    setup_root_claude_md(&worktree_path, repo_root)?;

    let mut worktree = Worktree::new(stage_id.to_string(), worktree_path, branch_name);
    worktree.mark_active();

    Ok(worktree)
}

/// Remove a worktree
///
/// Runs: git worktree remove .worktrees/{stage_id}
pub fn remove_worktree(stage_id: &str, repo_root: &Path, force: bool) -> Result<()> {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);

    if !worktree_path.exists() {
        bail!("Worktree does not exist: {}", worktree_path.display());
    }

    // Clean up settings and symlinks first
    cleanup_worktree_settings(&worktree_path);

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

/// Get an existing worktree or create a new one
///
/// If a valid worktree exists at .worktrees/{stage_id}/, reuses it.
/// If the directory exists but is not a valid worktree, removes it and recreates.
/// Otherwise, creates a new worktree.
///
/// If `base_branch` is Some(branch), new worktrees will branch from that branch.
/// If `base_branch` is None, new worktrees will branch from HEAD.
///
/// This function is idempotent and safe to call multiple times for the same stage.
pub fn get_or_create_worktree(
    stage_id: &str,
    repo_root: &Path,
    base_branch: Option<&str>,
) -> Result<Worktree> {
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
    create_worktree(stage_id, repo_root, base_branch)
}

/// Find a worktree by ID or prefix.
///
/// First attempts an exact match: `.worktrees/{id}`
/// If not found, scans the .worktrees directory for directories starting with the given prefix.
///
/// # Arguments
/// * `repo_root` - Path to the repository root
/// * `id` - The stage ID or prefix to find
///
/// # Returns
/// * `Ok(Some(path))` - Single match found (returns path to worktree directory)
/// * `Ok(None)` - No matches found
/// * `Err` - Multiple matches found (ambiguous prefix) or filesystem error
pub fn find_worktree_by_prefix(repo_root: &Path, id: &str) -> Result<Option<PathBuf>> {
    let worktrees_dir = repo_root.join(".worktrees");

    if !worktrees_dir.exists() {
        return Ok(None);
    }

    // Try exact match first
    let exact_path = worktrees_dir.join(id);
    if exact_path.exists() && exact_path.is_dir() {
        return Ok(Some(exact_path));
    }

    // Scan for prefix matches
    let entries = fs::read_dir(&worktrees_dir).with_context(|| {
        format!(
            "Failed to read worktrees directory: {}",
            worktrees_dir.display()
        )
    })?;

    let mut matches: Vec<PathBuf> = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.starts_with(id) {
                matches.push(path);
            }
        }
    }

    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.into_iter().next().unwrap())),
        _ => {
            let match_names: Vec<String> = matches
                .iter()
                .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(String::from))
                .collect();
            bail!(
                "Ambiguous worktree prefix '{}': matches {} worktrees ({})",
                id,
                matches.len(),
                match_names.join(", ")
            );
        }
    }
}

/// Extract stage ID from a worktree path.
///
/// # Arguments
/// * `path` - Path to the worktree directory
///
/// # Returns
/// The stage ID (directory name)
pub fn extract_worktree_stage_id(path: &Path) -> Option<String> {
    path.file_name().and_then(|s| s.to_str()).map(String::from)
}

/// Extract stage ID from any path that contains `.worktrees/<stage-id>`.
///
/// This works for paths at any depth within a worktree:
/// - `.worktrees/my-stage` -> Some("my-stage")
/// - `/home/user/project/.worktrees/my-stage/src/main.rs` -> Some("my-stage")
/// - `/regular/path/without/worktree` -> None
///
/// # Arguments
/// * `path` - Any path that may be inside a worktree
///
/// # Returns
/// The stage ID if the path is within a `.worktrees/<stage-id>` directory, None otherwise.
pub fn extract_stage_id_from_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();

    // Look for ".worktrees/" pattern in the path
    let worktrees_marker = ".worktrees/";
    if let Some(idx) = path_str.find(worktrees_marker) {
        let after_worktrees = &path_str[idx + worktrees_marker.len()..];
        // Take everything up to the next path separator (or end of string)
        let stage_id = after_worktrees
            .split(std::path::MAIN_SEPARATOR)
            .next()
            .filter(|s| !s.is_empty())?;
        return Some(stage_id.to_string());
    }
    None
}

/// Find the worktree root directory from any path within that worktree.
///
/// Given a path like `/home/user/project/.worktrees/my-stage/src/lib/module.rs`,
/// returns `/home/user/project/.worktrees/my-stage`.
///
/// # Arguments
/// * `cwd` - Current working directory or any path within a worktree
///
/// # Returns
/// * `Some(PathBuf)` - Absolute path to the worktree root if `cwd` is inside a worktree
/// * `None` - If `cwd` is not inside a `.worktrees/<stage-id>` directory
///
/// # Examples
///
/// ```ignore
/// let path = Path::new("/home/user/project/.worktrees/my-stage/src/main.rs");
/// assert_eq!(
///     find_worktree_root_from_cwd(path),
///     Some(PathBuf::from("/home/user/project/.worktrees/my-stage"))
/// );
/// ```
pub fn find_worktree_root_from_cwd(cwd: &Path) -> Option<PathBuf> {
    let path_str = cwd.to_string_lossy();

    // Look for ".worktrees/" pattern in the path
    let worktrees_marker = ".worktrees/";
    let idx = path_str.find(worktrees_marker)?;

    let after_worktrees = &path_str[idx + worktrees_marker.len()..];

    // Extract the stage_id (next path component after .worktrees/)
    let stage_id = after_worktrees
        .split(std::path::MAIN_SEPARATOR)
        .next()
        .filter(|s| !s.is_empty())?;

    // Construct the worktree root path: everything up to and including .worktrees/stage_id
    let worktree_root_str = format!(
        "{}{}{}",
        &path_str[..idx],
        worktrees_marker,
        stage_id
    );

    let worktree_root = PathBuf::from(&worktree_root_str);

    // Attempt to canonicalize if the path exists, otherwise return the constructed path
    // This handles both absolute and relative input paths
    if worktree_root.exists() {
        worktree_root.canonicalize().ok()
    } else {
        // For testing with non-existent paths, return the constructed path
        Some(worktree_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_stage_id_from_path_deep_nesting() {
        let path = PathBuf::from("/home/user/project/.worktrees/my-stage/src/lib/module.rs");
        assert_eq!(
            extract_stage_id_from_path(&path),
            Some("my-stage".to_string())
        );
    }

    #[test]
    fn test_extract_stage_id_from_path_at_root() {
        let path = PathBuf::from("/home/user/project/.worktrees/test-stage");
        assert_eq!(
            extract_stage_id_from_path(&path),
            Some("test-stage".to_string())
        );
    }

    #[test]
    fn test_extract_stage_id_from_path_relative() {
        let path = PathBuf::from(".worktrees/stage-1/src/main.rs");
        assert_eq!(
            extract_stage_id_from_path(&path),
            Some("stage-1".to_string())
        );
    }

    #[test]
    fn test_extract_stage_id_from_path_not_in_worktree() {
        let path = PathBuf::from("/home/user/project/src/main.rs");
        assert_eq!(extract_stage_id_from_path(&path), None);
    }

    #[test]
    fn test_extract_stage_id_from_path_empty_stage_id() {
        let path = PathBuf::from("/home/user/project/.worktrees/");
        assert_eq!(extract_stage_id_from_path(&path), None);
    }

    #[test]
    fn test_find_worktree_root_from_cwd_deep_nesting() {
        let path = PathBuf::from("/home/user/project/.worktrees/my-stage/src/lib/module.rs");
        assert_eq!(
            find_worktree_root_from_cwd(&path),
            Some(PathBuf::from("/home/user/project/.worktrees/my-stage"))
        );
    }

    #[test]
    fn test_find_worktree_root_from_cwd_at_root() {
        let path = PathBuf::from("/home/user/project/.worktrees/test-stage");
        assert_eq!(
            find_worktree_root_from_cwd(&path),
            Some(PathBuf::from("/home/user/project/.worktrees/test-stage"))
        );
    }

    #[test]
    fn test_find_worktree_root_from_cwd_relative() {
        let path = PathBuf::from(".worktrees/stage-1/src/main.rs");
        assert_eq!(
            find_worktree_root_from_cwd(&path),
            Some(PathBuf::from(".worktrees/stage-1"))
        );
    }

    #[test]
    fn test_find_worktree_root_from_cwd_not_in_worktree() {
        let path = PathBuf::from("/home/user/project/src/main.rs");
        assert_eq!(find_worktree_root_from_cwd(&path), None);
    }

    #[test]
    fn test_find_worktree_root_from_cwd_empty_stage_id() {
        let path = PathBuf::from("/home/user/project/.worktrees/");
        assert_eq!(find_worktree_root_from_cwd(&path), None);
    }

    #[test]
    fn test_find_worktree_root_from_cwd_with_trailing_slash() {
        let path = PathBuf::from("/home/user/project/.worktrees/my-stage/");
        // The trailing slash means the path ends after the stage_id, so it should still work
        let result = find_worktree_root_from_cwd(&path);
        assert_eq!(
            result,
            Some(PathBuf::from("/home/user/project/.worktrees/my-stage"))
        );
    }
}
