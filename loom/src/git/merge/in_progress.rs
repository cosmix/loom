//! Active merge detection for the main repo and per-stage worktrees.
//!
//! `MERGE_HEAD` in `.git/` (or in the resolved gitdir for worktrees) signals a
//! merge currently in progress. Recovery and routing code uses these helpers
//! as the single source of truth — never inspect `.git/MERGE_HEAD` ad-hoc.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::git::runner::run_git_checked;

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

/// Inspect a single repo or worktree path, returning the merge if present.
///
/// `is_worktree=true` means the caller knows this is a worktree directory
/// (relevant for distinguishing the `MergeLocation` variant).
fn detect_at(repo_path: &Path, is_worktree: bool) -> Result<Option<InProgressMerge>> {
    let git_dir = match git_dir_for_repo_path(repo_path) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

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

    let unmerged = unmerged_paths(repo_path)?;
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
    detect_at(repo_path, false)
}

/// Inspect a worktree path, returning the active merge if present.
pub fn detect_in_progress_merge_at_worktree(
    worktree_path: &Path,
) -> Result<Option<InProgressMerge>> {
    detect_at(worktree_path, true)
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

/// Run `git diff --name-only --diff-filter=U` in the given path and return
/// its lines. Returns an empty Vec on git failure (the caller treats the
/// "no unmerged paths" case as `ResolvedButUncommitted`).
fn unmerged_paths(repo_path: &Path) -> Result<Vec<String>> {
    let stdout = run_git_checked(&["diff", "--name-only", "--diff-filter=U"], repo_path)?;
    Ok(stdout
        .lines()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(args: &[&str], cwd: &Path) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        run_git(&["init", "-b", "main"], root);
        run_git(&["config", "user.email", "t@t.com"], root);
        run_git(&["config", "user.name", "t"], root);
        std::fs::write(root.join("README.md"), "seed").unwrap();
        run_git(&["add", "README.md"], root);
        run_git(&["commit", "-m", "seed"], root);
        tmp
    }

    #[test]
    fn merge_head_exists_returns_false_when_clean() {
        let tmp = init_repo();
        assert!(!merge_head_exists(tmp.path()).unwrap());
    }

    #[test]
    fn merge_head_exists_returns_true_when_active() {
        let tmp = init_repo();
        let root = tmp.path();
        // Build a conflicting branch.
        run_git(&["checkout", "-b", "feature"], root);
        std::fs::write(root.join("a.txt"), "branch").unwrap();
        run_git(&["add", "a.txt"], root);
        run_git(&["commit", "-m", "branch"], root);
        run_git(&["checkout", "main"], root);
        std::fs::write(root.join("a.txt"), "main").unwrap();
        run_git(&["add", "a.txt"], root);
        run_git(&["commit", "-m", "main"], root);

        // Try to merge — produces MERGE_HEAD with conflict.
        let out = Command::new("git")
            .args(["merge", "--no-ff", "feature"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(!out.status.success());

        assert!(merge_head_exists(root).unwrap());
    }

    #[test]
    fn detect_returns_unmerged_paths_state_when_conflicts_present() {
        let tmp = init_repo();
        let root = tmp.path();
        run_git(&["checkout", "-b", "feature"], root);
        std::fs::write(root.join("file.txt"), "feature\n").unwrap();
        run_git(&["add", "file.txt"], root);
        run_git(&["commit", "-m", "feature"], root);
        run_git(&["checkout", "main"], root);
        std::fs::write(root.join("file.txt"), "main\n").unwrap();
        run_git(&["add", "file.txt"], root);
        run_git(&["commit", "-m", "main"], root);

        let _ = Command::new("git")
            .args(["merge", "--no-ff", "feature"])
            .current_dir(root)
            .output()
            .unwrap();

        let merge = detect_in_progress_merge_at(root).unwrap().unwrap();
        match merge.state {
            ActiveMergeState::HasUnmergedPaths(paths) => {
                assert!(paths.iter().any(|p| p == "file.txt"));
            }
            other => panic!("expected HasUnmergedPaths, got {other:?}"),
        }
        assert!(matches!(merge.location, MergeLocation::MainRepo { .. }));
        assert_eq!(merge.merge_heads.len(), 1);
    }

    #[test]
    fn detect_returns_resolved_but_uncommitted_when_paths_staged() {
        let tmp = init_repo();
        let root = tmp.path();
        run_git(&["checkout", "-b", "feature"], root);
        std::fs::write(root.join("file.txt"), "feature\n").unwrap();
        run_git(&["add", "file.txt"], root);
        run_git(&["commit", "-m", "feature"], root);
        run_git(&["checkout", "main"], root);
        std::fs::write(root.join("file.txt"), "main\n").unwrap();
        run_git(&["add", "file.txt"], root);
        run_git(&["commit", "-m", "main"], root);

        let _ = Command::new("git")
            .args(["merge", "--no-ff", "feature"])
            .current_dir(root)
            .output()
            .unwrap();

        // Resolve by writing one side and staging.
        std::fs::write(root.join("file.txt"), "resolved\n").unwrap();
        run_git(&["add", "file.txt"], root);

        let merge = detect_in_progress_merge_at(root).unwrap().unwrap();
        assert_eq!(merge.state, ActiveMergeState::ResolvedButUncommitted);
    }

    #[test]
    fn merge_heads_parses_octopus_multiple_lines() {
        let tmp = init_repo();
        let root = tmp.path();
        let git_dir = root.join(".git");
        // Write a fake MERGE_HEAD with two lines to test parser.
        std::fs::write(
            git_dir.join("MERGE_HEAD"),
            "abc1234567890abcdef1234567890abcdef1234\n\
             def4567890abcdef1234567890abcdef1234abcd\n",
        )
        .unwrap();

        let merge = detect_in_progress_merge_at(root).unwrap().unwrap();
        assert_eq!(merge.merge_heads.len(), 2);

        // Cleanup the manufactured MERGE_HEAD.
        std::fs::remove_file(git_dir.join("MERGE_HEAD")).ok();
    }

    #[test]
    fn git_dir_for_repo_resolves_dot_git_directory() {
        let tmp = init_repo();
        let root = tmp.path();
        let git_dir = git_dir_for_repo_path(root).unwrap();
        assert!(git_dir.is_dir());
        assert_eq!(git_dir, root.join(".git"));
    }

    #[test]
    fn git_dir_for_repo_resolves_dot_git_file_with_absolute_path() {
        let tmp = TempDir::new().unwrap();
        let worktree_path = tmp.path().join("wt");
        std::fs::create_dir_all(&worktree_path).unwrap();
        let absolute_gitdir = tmp.path().join("gits").join("wt-gitdir");
        std::fs::create_dir_all(&absolute_gitdir).unwrap();
        std::fs::write(
            worktree_path.join(".git"),
            format!("gitdir: {}\n", absolute_gitdir.display()),
        )
        .unwrap();

        let resolved = git_dir_for_repo_path(&worktree_path).unwrap();
        assert_eq!(resolved, absolute_gitdir);
    }

    #[test]
    fn git_dir_for_repo_resolves_dot_git_file_with_relative_path() {
        let tmp = TempDir::new().unwrap();
        let worktree_path = tmp.path().join("wt");
        std::fs::create_dir_all(&worktree_path).unwrap();
        let target = worktree_path.join("..").join("real-gitdir");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(worktree_path.join(".git"), "gitdir: ../real-gitdir\n").unwrap();

        let resolved = git_dir_for_repo_path(&worktree_path).unwrap();
        assert_eq!(
            resolved.canonicalize().unwrap(),
            target.canonicalize().unwrap()
        );
    }

    #[test]
    fn git_dir_for_repo_errors_on_malformed_dot_git_file() {
        let tmp = TempDir::new().unwrap();
        let worktree_path = tmp.path().join("wt");
        std::fs::create_dir_all(&worktree_path).unwrap();
        std::fs::write(worktree_path.join(".git"), "garbage\n").unwrap();

        let result = git_dir_for_repo_path(&worktree_path);
        assert!(result.is_err(), "malformed .git file must produce an error");
    }

    #[test]
    fn detect_returns_none_when_not_a_repo() {
        let tmp = TempDir::new().unwrap();
        let result = detect_in_progress_merge_at(tmp.path()).unwrap();
        assert!(result.is_none());
    }
}
