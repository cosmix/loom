//! Tests for half-removed worktree recovery and removal-failure messages.

use super::{blocking_paths, removal_failure, remove_empty_scaffold_dirs, unknown_entries};
use crate::git::cleanup::tests::{git_ok, setup_git_repo};
use crate::git::cleanup::{cleanup_after_merge, cleanup_worktree, CleanupConfig};
use serial_test::serial;
use std::fs;
use std::os::unix::fs::symlink;
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
fn half_removed_worktree_with_unknown_empty_directory_is_refused_untouched() {
    let (repo, worktree) = half_removed_repo();
    fs::create_dir(worktree.join("user-directory")).unwrap();

    let err = cleanup_worktree("stage-1", repo.path(), false).unwrap_err();
    assert!(format!("{err:#}").contains("user-directory"));
    assert!(worktree.join("user-directory").is_dir());
    assert!(worktree.join(".claude/.cc-writes").is_dir());
}

#[test]
#[serial]
fn half_removed_worktree_preserves_settings_left_without_git_ownership() {
    let (repo, worktree) = half_removed_repo();
    let settings = worktree.join(".claude/settings.json");
    fs::write(&settings, "user changes").unwrap();

    let err = cleanup_worktree("stage-1", repo.path(), false).unwrap_err();
    assert!(format!("{err:#}").contains(".claude/settings.json"));
    assert_eq!(fs::read_to_string(settings).unwrap(), "user changes");
    assert!(worktree.join(".claude/.cc-writes").is_dir());
}

#[test]
#[serial]
fn half_removed_worktree_preserves_scaffold_named_symlink() {
    let (repo, worktree) = half_removed_repo();
    let target = repo.path().join("user-target");
    fs::write(&target, "keep").unwrap();
    symlink(&target, worktree.join("CLAUDE.md")).unwrap();

    let err = cleanup_worktree("stage-1", repo.path(), false).unwrap_err();
    assert!(format!("{err:#}").contains("CLAUDE.md"));
    assert!(fs::symlink_metadata(worktree.join("CLAUDE.md"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(target).unwrap(), "keep");
}

#[test]
#[serial]
fn half_removed_worktree_preserves_empty_tool_name() {
    let (repo, worktree) = half_removed_repo();
    let stub = worktree.join(".bashrc");
    fs::write(&stub, "").unwrap();

    let err = cleanup_worktree("stage-1", repo.path(), false).unwrap_err();
    assert!(format!("{err:#}").contains(".bashrc"));
    assert!(stub.is_file());
    assert_eq!(fs::metadata(stub).unwrap().len(), 0);
}

#[test]
fn late_file_prevents_directory_only_removal() {
    let (_repo, worktree) = half_removed_repo();
    assert!(unknown_entries(&worktree).unwrap().is_empty());
    let late = worktree.join(".claude/.cc-writes/late.txt");
    fs::write(&late, "keep").unwrap();

    assert!(remove_empty_scaffold_dirs(&worktree).is_err());
    assert_eq!(fs::read_to_string(late).unwrap(), "keep");
    assert!(worktree.is_dir());
}

#[test]
fn swapped_scaffold_parent_does_not_redirect_removal() {
    let (repo, worktree) = half_removed_repo();
    assert!(unknown_entries(&worktree).unwrap().is_empty());
    let outside = repo.path().join("outside");
    fs::create_dir_all(outside.join(".cc-writes")).unwrap();
    fs::remove_dir(worktree.join(".claude/.cc-writes")).unwrap();
    fs::remove_dir(worktree.join(".claude")).unwrap();
    symlink(&outside, worktree.join(".claude")).unwrap();

    assert!(remove_empty_scaffold_dirs(&worktree).is_err());
    assert!(outside.join(".cc-writes").is_dir());
    assert!(worktree.join(".claude").is_symlink());
}

#[test]
fn failed_remnant_inspection_does_not_report_an_empty_tree() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("unreadable-as-directory");
    fs::write(&file, "keep").unwrap();

    assert!(unknown_entries(&file).is_err());
    assert_eq!(fs::read_to_string(file).unwrap(), "keep");
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
