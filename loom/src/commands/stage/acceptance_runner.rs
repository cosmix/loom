//! Acceptance criteria directory resolution
//!
//! This module provides helpers for resolving the working directory
//! where acceptance criteria should be executed.

use std::path::{Path, PathBuf};

/// Resolve acceptance directory from worktree root and working_dir.
///
/// # Arguments
/// * `worktree_root` - The root of the worktree (e.g., ".worktrees/stage-id")
/// * `working_dir` - The stage's working_dir setting (e.g., ".", "loom", None)
///
/// # Returns
/// The resolved path for running acceptance criteria
pub fn resolve_acceptance_dir(
    worktree_root: Option<&Path>,
    working_dir: Option<&str>,
) -> Option<PathBuf> {
    match (worktree_root, working_dir) {
        (Some(root), Some(subdir)) => {
            // Handle "." special case - use worktree root directly
            if subdir == "." {
                Some(root.to_path_buf())
            } else {
                let full_path = root.join(subdir);
                if full_path.exists() {
                    Some(full_path)
                } else {
                    // Fall back to worktree root if subdirectory doesn't exist
                    eprintln!(
                        "Warning: stage working_dir '{subdir}' does not exist in worktree at '{}', using worktree root",
                        full_path.display()
                    );
                    Some(root.to_path_buf())
                }
            }
        }
        (Some(root), None) => {
            // No working_dir specified, use worktree root
            Some(root.to_path_buf())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_acceptance_dir_dot_uses_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("."));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_subdir_uses_worktree_root_joined() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Create the subdirectory
        let subdir_path = worktree_root.join("loom");
        std::fs::create_dir_all(&subdir_path).unwrap();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("loom"));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), subdir_path);
    }

    #[test]
    fn test_resolve_acceptance_dir_missing_subdir_falls_back_to_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Don't create the subdirectory - it should fall back to root
        let result = resolve_acceptance_dir(Some(worktree_root), Some("nonexistent"));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_none_working_dir_uses_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        let result = resolve_acceptance_dir(Some(worktree_root), None);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_no_worktree_returns_none() {
        let result = resolve_acceptance_dir(None, Some("."));

        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_acceptance_dir_nested_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Create a nested subdirectory
        let subdir_path = worktree_root.join("packages/core");
        std::fs::create_dir_all(&subdir_path).unwrap();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("packages/core"));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), subdir_path);
    }
}
