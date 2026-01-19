//! Path resolution utilities for worktrees

use std::path::{Path, PathBuf};

/// Find the main repository root from any path within a worktree or the main repo.
///
/// Given a path like `/home/user/project/.worktrees/my-stage/src/lib/module.rs`,
/// returns `/home/user/project` (the main repo root, not the worktree root).
///
/// If the path is already in the main repo (not in a worktree), returns the path
/// after attempting to canonicalize it.
///
/// # Arguments
/// * `cwd` - Current working directory or any path within a worktree or main repo
///
/// # Returns
/// * `Some(PathBuf)` - Path to the main repository root
/// * `None` - If the path cannot be resolved
///
/// # Examples
///
/// ```ignore
/// // From within a worktree
/// let path = Path::new("/home/user/project/.worktrees/my-stage/src/main.rs");
/// assert_eq!(
///     find_repo_root_from_cwd(path),
///     Some(PathBuf::from("/home/user/project"))
/// );
///
/// // From within the main repo
/// let path = Path::new("/home/user/project/src/main.rs");
/// assert_eq!(
///     find_repo_root_from_cwd(path),
///     Some(PathBuf::from("/home/user/project"))
/// );
/// ```
pub fn find_repo_root_from_cwd(cwd: &Path) -> Option<PathBuf> {
    let path_str = cwd.to_string_lossy();

    // Check if we're inside a worktree by looking for ".worktrees/" pattern
    let worktrees_marker = ".worktrees/";
    if let Some(idx) = path_str.find(worktrees_marker) {
        // Extract the repo root (everything before .worktrees/)
        let repo_root_str = &path_str[..idx];
        let repo_root = PathBuf::from(repo_root_str.trim_end_matches('/'));

        // Attempt to canonicalize if the path exists
        if repo_root.exists() {
            return repo_root.canonicalize().ok();
        } else {
            // For testing with non-existent paths, return the constructed path
            return Some(repo_root);
        }
    }

    // Not in a worktree - try to find git repo root by looking for .git directory
    // Walk up the directory tree looking for .git
    let mut current = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(cwd)
    };

    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return current.canonicalize().ok().or(Some(current));
        }

        if !current.pop() {
            break;
        }
    }

    // Fallback: return the original cwd if nothing else works
    cwd.canonicalize().ok()
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
    let worktree_root_str = format!("{}{}{}", &path_str[..idx], worktrees_marker, stage_id);

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

    // Tests for find_repo_root_from_cwd

    #[test]
    fn test_find_repo_root_from_cwd_in_worktree() {
        let path = PathBuf::from("/home/user/project/.worktrees/my-stage/src/lib/module.rs");
        assert_eq!(
            find_repo_root_from_cwd(&path),
            Some(PathBuf::from("/home/user/project"))
        );
    }

    #[test]
    fn test_find_repo_root_from_cwd_at_worktree_root() {
        let path = PathBuf::from("/home/user/project/.worktrees/test-stage");
        assert_eq!(
            find_repo_root_from_cwd(&path),
            Some(PathBuf::from("/home/user/project"))
        );
    }

    #[test]
    fn test_find_repo_root_from_cwd_relative_worktree() {
        let path = PathBuf::from(".worktrees/stage-1/src/main.rs");
        // For relative paths in worktree, should return empty path (before .worktrees)
        // which becomes an empty PathBuf
        let result = find_repo_root_from_cwd(&path);
        assert_eq!(result, Some(PathBuf::from("")));
    }

    #[test]
    fn test_find_repo_root_from_cwd_with_trailing_slash() {
        let path = PathBuf::from("/home/user/project/.worktrees/my-stage/");
        assert_eq!(
            find_repo_root_from_cwd(&path),
            Some(PathBuf::from("/home/user/project"))
        );
    }
}
