//! Active merge detection for the main repo and per-stage worktrees.
//!
//! `MERGE_HEAD` in `.git/` (or in the resolved gitdir for worktrees) signals a
//! merge currently in progress. Recovery and routing code uses these helpers
//! as the single source of truth — never inspect `.git/MERGE_HEAD` ad-hoc.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::git::runner::run_git_checked;
use crate::git::worktree::WorktreeGit;

const UNMERGED_PATHS_ARGS: [&str; 3] = ["diff", "--name-only", "--diff-filter=U"];

/// Where an in-progress merge was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeLocation {
    /// Main repo: `.git/MERGE_HEAD` set on `repo_path`.
    MainRepo {
        repo_path: PathBuf,
        git_dir: PathBuf,
    },
    /// Worktree: `.git` is a file pointing to `git_dir` which contains `MERGE_HEAD`.
    Worktree {
        worktree_path: PathBuf,
        git_dir: PathBuf,
    },
}

/// State of an active merge: still has unmerged paths, or all conflicts staged
/// but not yet committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveMergeState {
    /// `git diff --name-only --diff-filter=U` returned at least one path.
    HasUnmergedPaths(Vec<String>),
    /// `MERGE_HEAD` set but no unmerged paths — resolver staged but did not commit.
    ResolvedButUncommitted,
}

/// An active merge detected on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InProgressMerge {
    pub location: MergeLocation,
    /// SHA(s) read from `MERGE_HEAD` (octopus merges produce more than one).
    pub merge_heads: Vec<String>,
    pub state: ActiveMergeState,
}

impl InProgressMerge {
    /// Display-friendly path for the merge, used in user-facing error messages.
    pub fn location_path(&self) -> &Path {
        match &self.location {
            MergeLocation::MainRepo { repo_path, .. } => repo_path,
            MergeLocation::Worktree { worktree_path, .. } => worktree_path,
        }
    }
}

/// Resolve the `.git` directory for any repo or worktree path.
///
/// Handles `.git` as a directory or as a file containing `gitdir: <path>`,
/// resolving relative gitdir paths against the directory containing `.git`.
pub fn git_dir_for_repo_path(repo_path: &Path) -> Result<PathBuf> {
    let dot_git = repo_path.join(".git");
    let metadata = std::fs::metadata(&dot_git)
        .with_context(|| format!("Cannot stat {}", dot_git.display()))?;

    if metadata.is_dir() {
        return Ok(dot_git);
    }

    if !metadata.is_file() {
        anyhow::bail!(
            "Unexpected .git type at {} (neither file nor directory)",
            dot_git.display()
        );
    }

    let content = std::fs::read_to_string(&dot_git)
        .with_context(|| format!("Cannot read {}", dot_git.display()))?;
    let gitdir_line = content
        .lines()
        .find_map(|l| l.strip_prefix("gitdir:"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Malformed .git file at {} (missing 'gitdir:' header)",
                dot_git.display()
            )
        })?
        .trim();

    let gitdir = PathBuf::from(gitdir_line);
    let resolved = if gitdir.is_absolute() {
        gitdir
    } else {
        // Relative gitdirs are relative to the directory containing the `.git` file.
        repo_path.join(gitdir)
    };
    Ok(resolved)
}

/// Cheap predicate: is there a `MERGE_HEAD` at this repo or worktree path?
pub fn merge_head_exists(repo_path: &Path) -> Result<bool> {
    let git_dir = match git_dir_for_repo_path(repo_path) {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    Ok(git_dir.join("MERGE_HEAD").exists())
}

/// The merge at `repo_path` whose git directory is `git_dir`, if present.
/// `unmerged` lists its conflicted paths.
fn detect_at(
    repo_path: &Path,
    git_dir: PathBuf,
    is_worktree: bool,
    unmerged: impl FnOnce() -> Result<String>,
) -> Result<Option<InProgressMerge>> {
    let merge_head_path = git_dir.join("MERGE_HEAD");
    if !merge_head_path.exists() {
        return Ok(None);
    }

    let merge_heads: Vec<String> = std::fs::read_to_string(&merge_head_path)
        .with_context(|| format!("Cannot read {}", merge_head_path.display()))?
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let listing = unmerged()?;
    let unmerged: Vec<String> = listing
        .lines()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let state = if unmerged.is_empty() {
        ActiveMergeState::ResolvedButUncommitted
    } else {
        ActiveMergeState::HasUnmergedPaths(unmerged)
    };

    let location = if is_worktree {
        MergeLocation::Worktree {
            worktree_path: repo_path.to_path_buf(),
            git_dir,
        }
    } else {
        MergeLocation::MainRepo {
            repo_path: repo_path.to_path_buf(),
            git_dir,
        }
    };

    Ok(Some(InProgressMerge {
        location,
        merge_heads,
        state,
    }))
}

/// Inspect a single main-repo path, returning the active merge if present.
pub fn detect_in_progress_merge_at(repo_path: &Path) -> Result<Option<InProgressMerge>> {
    let Ok(git_dir) = git_dir_for_repo_path(repo_path) else {
        return Ok(None);
    };
    detect_at(repo_path, git_dir, false, || {
        run_git_checked(&UNMERGED_PATHS_ARGS, repo_path)
    })
}

/// Inspect a stage worktree, returning the active merge if present. A stage
/// session can rewrite the worktree's `.git`, so its git directory comes from
/// the registry of the repository the path sits in, and a worktree that
/// registry does not name is an error.
pub fn detect_in_progress_merge_at_worktree(
    worktree_path: &Path,
) -> Result<Option<InProgressMerge>> {
    let repo_root = worktree_path
        .parent()
        .filter(|parent| parent.file_name() == Some(std::ffi::OsStr::new(".worktrees")))
        .and_then(Path::parent)
        .with_context(|| format!("{} is not a stage worktree", worktree_path.display()))?;
    let git = WorktreeGit::pinned(repo_root, worktree_path)?;
    let git_dir = git
        .git_dir_path()
        .context("a pinned worktree has a git directory")?
        .to_path_buf();
    detect_at(worktree_path, git_dir, true, || {
        git.run_checked(&UNMERGED_PATHS_ARGS)
    })
}

/// Inspect main repo and the stage's worktree, returning all active merges found.
pub fn detect_in_progress_merges(stage_id: &str, repo_root: &Path) -> Vec<InProgressMerge> {
    let mut merges = Vec::new();

    if let Ok(Some(m)) = detect_in_progress_merge_at(repo_root) {
        merges.push(m);
    }

    let worktree = repo_root.join(".worktrees").join(stage_id);
    if worktree.exists() {
        if let Ok(Some(m)) = detect_in_progress_merge_at_worktree(&worktree) {
            merges.push(m);
        }
    }

    merges
}

#[cfg(test)]
#[path = "in_progress_tests.rs"]
mod tests;
