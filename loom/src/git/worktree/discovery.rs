//! Worktree discovery and lookup operations

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

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
}
