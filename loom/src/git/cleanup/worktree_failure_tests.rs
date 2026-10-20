//! Tests for half-removed worktree recovery and removal-failure messages.

use super::{blocking_paths, removal_failure};
use crate::git::cleanup::tests::{git_ok, setup_git_repo};
use crate::git::cleanup::{cleanup_after_merge, cleanup_worktree, CleanupConfig};
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Repo with `.worktrees/stage-1` in the state git leaves after `worktree
/// remove` loses the race with a late write: admin dir and tracked files gone,
/// only `.claude/.cc-writes/` on disk.
fn half_removed_repo() -> (TempDir, PathBuf) {
    let repo = setup_git_repo();
    fs::create_dir_all(repo.path().join(".worktrees")).unwrap();
    git_ok(
        repo.path(),
        &[
            "worktree",
            "add",
            ".worktrees/stage-1",
            "-b",
            "loom/stage-1",
        ],
    );
    let worktree = repo.path().join(".worktrees/stage-1");
    fs::remove_dir_all(&worktree).unwrap();
    fs::remove_dir_all(repo.path().join(".git/worktrees/stage-1")).unwrap();
    fs::create_dir_all(worktree.join(".claude/.cc-writes")).unwrap();
    (repo, worktree)
}

fn branch_exists(repo: &std::path::Path) -> bool {
    crate::git::branch::branch_exists("loom/stage-1", repo).unwrap_or(false)
}

#[test]
#[serial]
fn half_removed_worktree_with_only_scaffold_is_cleaned() {
    let (repo, worktree) = half_removed_repo();
    let result = cleanup_after_merge("stage-1", repo.path(), &CleanupConfig::quiet())
        .expect("cleanup should recover");
    assert!(result.worktree_removed);
    assert!(!worktree.exists());
    assert!(!branch_exists(repo.path()), "branch should be deleted");
}

#[test]
#[serial]
fn half_removed_worktree_with_regular_file_is_refused_untouched() {
    let (repo, worktree) = half_removed_repo();
    fs::write(worktree.join("notes.txt"), "keep").unwrap();
    let err = cleanup_worktree("stage-1", repo.path(), false).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("notes.txt"), "got: {message}");
    assert!(worktree.join("notes.txt").exists());
    assert!(worktree.join(".claude/.cc-writes").exists());
}

#[test]
#[serial]
fn blocking_paths_without_git_dir_ignores_enclosing_repo() {
    let (repo, worktree) = half_removed_repo();
    fs::write(repo.path().join("README.md"), "dirty").unwrap();
    fs::write(repo.path().join("main-only.txt"), "untracked").unwrap();
    fs::write(worktree.join("notes.txt"), "x").unwrap();
    let listing = blocking_paths(&worktree);
    assert!(listing.contains("notes.txt"), "got: {listing}");
    assert!(!listing.contains("main-only.txt"), "got: {listing}");
    assert!(!listing.contains("README.md"), "got: {listing}");
}

#[test]
fn non_refusal_failure_headline_does_not_blame_force() {
    let dir = TempDir::new().unwrap();
    let err = removal_failure(dir.path(), anyhow::anyhow!("fatal: something else"));
    let message = format!("{err:#}");
    assert!(message.contains("git worktree remove failed for"));
    assert!(!message.contains("force was not set"), "got: {message}");
}
