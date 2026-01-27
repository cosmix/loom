//! Work directory integrity validation
//!
//! This module provides validation to detect and prevent corruption of the .work
//! directory, particularly from accidental commits of the .work symlink in worktrees.

use anyhow::{bail, Result};
use std::path::Path;

/// State of the .work directory
#[derive(Debug, Clone, PartialEq)]
pub enum WorkDirState {
    /// .work is a regular directory (correct state for main repo)
    Directory,
    /// .work is a symlink (correct state for worktrees)
    Symlink { target: String },
    /// .work does not exist
    Missing,
    /// .work exists but is neither directory nor symlink (corrupted)
    Invalid,
}

impl std::fmt::Display for WorkDirState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkDirState::Directory => write!(f, "directory"),
            WorkDirState::Symlink { target } => write!(f, "symlink -> {}", target),
            WorkDirState::Missing => write!(f, "missing"),
            WorkDirState::Invalid => write!(f, "invalid"),
        }
    }
}

/// Check the current state of the .work directory
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// The current state of the .work directory
pub fn check_work_dir_state(repo_root: &Path) -> WorkDirState {
    let work_path = repo_root.join(".work");

    if !work_path.exists() && !work_path.is_symlink() {
        return WorkDirState::Missing;
    }

    if work_path.is_symlink() {
        let target = std::fs::read_link(&work_path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        return WorkDirState::Symlink { target };
    }

    if work_path.is_dir() {
        return WorkDirState::Directory;
    }

    WorkDirState::Invalid
}

/// Check if we're currently in a worktree (not the main repository)
///
/// # Arguments
/// * `current_dir` - Current working directory
///
/// # Returns
/// true if in a worktree, false if in main repo
pub fn is_in_worktree(current_dir: &Path) -> bool {
    let path_str = current_dir.to_string_lossy();
    path_str.contains(".worktrees/")
}

/// Validate that the .work directory is in the expected state
///
/// In the main repository, .work should be a directory.
/// In a worktree, .work should be a symlink.
///
/// This function is called during `loom init` and `loom run` to detect
/// potential corruption from committed .work symlinks.
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// * `Ok(())` if the state is valid
/// * `Err` with detailed message if corrupted
pub fn validate_work_dir_state(repo_root: &Path) -> Result<()> {
    let state = check_work_dir_state(repo_root);
    let in_worktree = is_in_worktree(repo_root);

    match (&state, in_worktree) {
        // Main repo with directory - correct
        (WorkDirState::Directory, false) => Ok(()),
        // Main repo with missing .work - fine, will be created
        (WorkDirState::Missing, false) => Ok(()),
        // Worktree with symlink - correct
        (WorkDirState::Symlink { .. }, true) => Ok(()),
        // Worktree with missing .work - will be created as symlink
        (WorkDirState::Missing, true) => Ok(()),

        // Main repo with symlink - CORRUPTED!
        (WorkDirState::Symlink { target }, false) => {
            bail!(
                "\n\
                ============================================================\n\
                CRITICAL: .work directory is corrupted!\n\
                ============================================================\n\
                \n\
                The .work directory is a symlink (-> {target}) in the main repo.\n\
                This typically happens when .work from a worktree was committed.\n\
                \n\
                TO FIX:\n\
                1. Remove the symlink: rm .work\n\
                2. Run: loom init <your-plan> --clean\n\
                \n\
                Or run: loom repair --fix\n\
                \n\
                PREVENTION: Always use 'git add <specific-files>' instead of\n\
                'git add -A' or 'git add .' in worktrees.\n\
                ============================================================"
            );
        }

        // Worktree with directory instead of symlink - unusual but not fatal
        (WorkDirState::Directory, true) => {
            eprintln!(
                "Warning: .work is a directory in worktree (expected symlink). \
                 This may cause state inconsistencies."
            );
            Ok(())
        }

        // Invalid state
        (WorkDirState::Invalid, _) => {
            bail!(
                ".work exists but is neither a directory nor symlink. \
                 Remove it and run 'loom init' again."
            );
        }
    }
}

/// Check if .work is properly ignored by git
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// true if .work is ignored
pub fn is_work_dir_git_ignored(repo_root: &Path) -> bool {
    let gitignore_path = repo_root.join(".gitignore");
    if !gitignore_path.exists() {
        return false;
    }

    match std::fs::read_to_string(&gitignore_path) {
        Ok(content) => {
            // Check for both patterns
            content.lines().any(|line| {
                let trimmed = line.trim();
                trimmed == ".work/" || trimmed == ".work"
            })
        }
        Err(_) => false,
    }
}

/// Check if .worktrees is properly ignored by git
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// true if .worktrees is ignored
pub fn is_worktrees_git_ignored(repo_root: &Path) -> bool {
    let gitignore_path = repo_root.join(".gitignore");
    if !gitignore_path.exists() {
        return false;
    }

    match std::fs::read_to_string(&gitignore_path) {
        Ok(content) => {
            content.lines().any(|line| {
                let trimmed = line.trim();
                trimmed == ".worktrees/" || trimmed == ".worktrees"
            })
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_check_work_dir_state_missing() {
        let temp = TempDir::new().unwrap();
        let state = check_work_dir_state(temp.path());
        assert_eq!(state, WorkDirState::Missing);
    }

    #[test]
    fn test_check_work_dir_state_directory() {
        let temp = TempDir::new().unwrap();
        fs::create_dir(temp.path().join(".work")).unwrap();
        let state = check_work_dir_state(temp.path());
        assert_eq!(state, WorkDirState::Directory);
    }

    #[test]
    #[cfg(unix)]
    fn test_check_work_dir_state_symlink() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, temp.path().join(".work")).unwrap();

        let state = check_work_dir_state(temp.path());
        match state {
            WorkDirState::Symlink { .. } => (),
            other => panic!("Expected Symlink, got {:?}", other),
        }
    }

    #[test]
    fn test_is_in_worktree() {
        use std::path::PathBuf;

        assert!(is_in_worktree(&PathBuf::from("/foo/.worktrees/my-stage")));
        assert!(is_in_worktree(&PathBuf::from("/foo/.worktrees/my-stage/src")));
        assert!(!is_in_worktree(&PathBuf::from("/foo/bar")));
        assert!(!is_in_worktree(&PathBuf::from("/foo/worktrees/bar")));
    }

    #[test]
    fn test_validate_work_dir_state_main_repo_ok() {
        let temp = TempDir::new().unwrap();
        fs::create_dir(temp.path().join(".work")).unwrap();
        assert!(validate_work_dir_state(temp.path()).is_ok());
    }

    #[test]
    fn test_validate_work_dir_state_main_repo_missing_ok() {
        let temp = TempDir::new().unwrap();
        // No .work - should be ok (will be created)
        assert!(validate_work_dir_state(temp.path()).is_ok());
    }

    #[test]
    fn test_is_work_dir_git_ignored() {
        let temp = TempDir::new().unwrap();

        // No gitignore
        assert!(!is_work_dir_git_ignored(temp.path()));

        // With proper ignore
        fs::write(temp.path().join(".gitignore"), ".work/\n.work\n").unwrap();
        assert!(is_work_dir_git_ignored(temp.path()));
    }
}
