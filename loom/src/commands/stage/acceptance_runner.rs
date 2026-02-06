//! Acceptance criteria directory resolution
//!
//! This module provides helpers for resolving the working directory
//! where acceptance criteria should be executed.

use anyhow::{Context, Result};
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
) -> Result<Option<PathBuf>> {
    match (worktree_root, working_dir) {
        (Some(root), Some(subdir)) => {
            // Handle "." special case - use worktree root directly
            if subdir == "." {
                return Ok(Some(root.to_path_buf()));
            }

            let full_path = root.join(subdir);

            // Canonicalize and check containment for path traversal defense
            let canonical = full_path.canonicalize().with_context(|| {
                format!(
                    "Failed to resolve acceptance directory: {}",
                    full_path.display()
                )
            })?;

            // Defense-in-depth: verify resolved path is within worktree
            let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            if !canonical.starts_with(&canonical_root) {
                anyhow::bail!(
                    "Acceptance directory {} escapes worktree root {}",
                    canonical.display(),
                    canonical_root.display()
                );
            }

            Ok(Some(canonical))
        }
        (Some(root), None) => {
            // No working_dir specified, use worktree root
            Ok(Some(root.to_path_buf()))
        }
        _ => Ok(None),
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

        let result = resolve_acceptance_dir(Some(worktree_root), Some(".")).unwrap();

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

        let result = resolve_acceptance_dir(Some(worktree_root), Some("loom")).unwrap();

        assert!(result.is_some());
        // Canonicalize for comparison
        let expected = subdir_path.canonicalize().unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_resolve_acceptance_dir_missing_subdir_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Don't create the subdirectory - should return error
        let result = resolve_acceptance_dir(Some(worktree_root), Some("nonexistent"));

        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_acceptance_dir_none_working_dir_uses_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        let result = resolve_acceptance_dir(Some(worktree_root), None).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_no_worktree_returns_none() {
        let result = resolve_acceptance_dir(None, Some(".")).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_acceptance_dir_nested_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Create a nested subdirectory
        let subdir_path = worktree_root.join("packages/core");
        std::fs::create_dir_all(&subdir_path).unwrap();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("packages/core")).unwrap();

        assert!(result.is_some());
        // Canonicalize for comparison
        let expected = subdir_path.canonicalize().unwrap();
        assert_eq!(result.unwrap(), expected);
    }
}
