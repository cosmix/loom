//! Work directory integrity validation
//!
//! This module provides validation to detect and prevent corruption of the
//! state directory, particularly from accidental commits of a work-directory
//! symlink in worktrees. It inspects whichever on-disk layout is present —
//! the nested `.loom/work`, or, for a workspace that predates the move, the
//! legacy `.work` — via [`state_dir`], which mirrors the nested-first-then-
//! legacy precedence [`crate::fs::work_dir::WorkDir::new`] uses.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::fs::work_dir::Layout;

/// Choose which state directory the checks in this module inspect, and
/// report which layout it is.
///
/// Nested-first-then-legacy, matching [`crate::fs::work_dir::WorkDir::new`]:
/// the nested `.loom/work` wins when present in any form, otherwise the
/// legacy `.work` when present in any form, otherwise the nested path (loom
/// never creates a bare `.work`, so a repo with neither reports the nested
/// location as where one would be created).
///
/// This is deliberately the mirror image of
/// `work_dir::workspace_at`, which keys on `config.toml`
/// because it is choosing a *workspace* to read state from. This module
/// inspects a directory's physical SHAPE — including corrupted shapes (a
/// broken symlink, a stray file) that leave `config.toml` unreadable or
/// missing — so keying on `config.toml` here would make the detector blind
/// to exactly the corruption it exists to catch. Checking `.loom/work`
/// itself, never a bare `.loom/`, keeps `.loom/cache/` (written by `loom
/// map`) from registering as a workspace.
pub fn state_dir(repo_root: &Path) -> (PathBuf, Layout) {
    let nested = repo_root.join(".loom").join("work");
    // Both checks are required: `exists()` follows a symlink and reports
    // false for a BROKEN one, which is precisely the corruption this module
    // exists to detect — `is_symlink()` catches it without following.
    if nested.exists() || nested.is_symlink() {
        return (nested, Layout::Nested);
    }
    let legacy = repo_root.join(".work");
    if legacy.exists() || legacy.is_symlink() {
        return (legacy, Layout::Legacy);
    }
    (nested, Layout::Nested)
}

/// State of the state directory (`.loom/work`, or the legacy `.work` — see
/// [`state_dir`])
#[derive(Debug, Clone, PartialEq)]
pub enum WorkDirState {
    /// The state directory is a regular directory (correct state for main repo)
    Directory,
    /// The state directory is a symlink (correct state for worktrees)
    Symlink { target: String },
    /// The state directory does not exist
    Missing,
    /// The state directory exists but is neither directory nor symlink (corrupted)
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

/// Check the current state of the state directory (`.loom/work`, or the
/// legacy `.work` — see [`state_dir`])
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// The current state of the resolved state directory
pub fn check_work_dir_state(repo_root: &Path) -> WorkDirState {
    let (work_path, _layout) = state_dir(repo_root);

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

/// Validate that the state directory is in the expected state
///
/// In the main repository, the state directory should be a directory. In a
/// worktree, it should be a symlink. Whichever layout is on disk — nested
/// `.loom/work` or legacy `.work`, see [`state_dir`] — is the one inspected,
/// and its spelling is what any error message names.
///
/// This function is called during `loom init` and `loom run` to detect
/// potential corruption from committed work-directory symlinks.
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// * `Ok(())` if the state is valid
/// * `Err` with detailed message if corrupted
pub fn validate_work_dir_state(repo_root: &Path) -> Result<()> {
    let (_, layout) = state_dir(repo_root);
    let display_path = match layout {
        Layout::Nested => ".loom/work",
        Layout::Legacy => ".work",
    };
    let state = check_work_dir_state(repo_root);
    let in_worktree = is_in_worktree(repo_root);

    match (&state, in_worktree) {
        // Main repo with directory - correct
        (WorkDirState::Directory, false) => Ok(()),
        // Main repo with missing state dir - fine, will be created
        (WorkDirState::Missing, false) => Ok(()),
        // Worktree with symlink - correct
        (WorkDirState::Symlink { .. }, true) => Ok(()),
        // Worktree with missing state dir - will be created as symlink
        (WorkDirState::Missing, true) => Ok(()),

        // Main repo with symlink - CORRUPTED!
        (WorkDirState::Symlink { target }, false) => {
            bail!(
                "\n\
                ============================================================\n\
                CRITICAL: {display_path} directory is corrupted!\n\
                ============================================================\n\
                \n\
                The {display_path} directory is a symlink (-> {target}) in the main repo.\n\
                This typically happens when {display_path} from a worktree was committed.\n\
                \n\
                TO FIX:\n\
                1. Remove the symlink: rm {display_path}\n\
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
                "Warning: {display_path} is a directory in worktree (expected symlink). \
                 This may cause state inconsistencies."
            );
            Ok(())
        }

        // Invalid state
        (WorkDirState::Invalid, _) => {
            bail!(
                "{display_path} exists but is neither a directory nor symlink. \
                 Remove it and run 'loom init' again."
            );
        }
    }
}

/// Check if the state directory is properly ignored by git
///
/// Accepts whichever pair matches the resolved layout (see [`state_dir`]):
/// the nested `.loom/work/` / `.loom/work`, or, for a legacy workspace,
/// `.work/` / `.work`.
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// true if the state directory is ignored
pub fn is_work_dir_git_ignored(repo_root: &Path) -> bool {
    let gitignore_path = repo_root.join(".gitignore");
    if !gitignore_path.exists() {
        return false;
    }

    let (_, layout) = state_dir(repo_root);
    let (slash, bare) = match layout {
        Layout::Nested => (".loom/work/", ".loom/work"),
        Layout::Legacy => (".work/", ".work"),
    };

    match std::fs::read_to_string(&gitignore_path) {
        Ok(content) => content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == slash || trimmed == bare
        }),
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
        Ok(content) => content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == ".worktrees/" || trimmed == ".worktrees"
        }),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests;
