//! Tests for scaffold/cleanup handling of `.loom/memory-spool.jsonl`.
//!
//! Regression coverage for the bug where a stage that recorded a memory note
//! left a drained `.loom/memory-spool.jsonl` on disk, which made non-forced
//! `git worktree remove` refuse the worktree (untracked, and previously
//! nothing removed or ignored it).

use super::tests::{git_ok, setup_git_repo};
use crate::git::cleanup::cleanup_worktree;
use crate::git::cleanup::worktree::remove_worktree_scaffold;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_remove_worktree_scaffold_removes_drained_spool() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    let loom_dir = worktree_path.join(".loom");
    fs::create_dir_all(&loom_dir).unwrap();
    fs::write(loom_dir.join("memory-spool.jsonl"), "").unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(!loom_dir.join("memory-spool.jsonl").exists());
    assert!(!loom_dir.exists(), "empty .loom dir should be removed too");
}

#[test]
fn test_remove_worktree_scaffold_preserves_non_spool_loom_content() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    let loom_dir = worktree_path.join(".loom");
    fs::create_dir_all(&loom_dir).unwrap();
    fs::write(loom_dir.join("memory-spool.jsonl"), "").unwrap();
    fs::write(loom_dir.join("config.toml"), "key = 1").unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(
        !loom_dir.join("memory-spool.jsonl").exists(),
        "drained spool should still be removed"
    );
    assert!(
        loom_dir.join("config.toml").exists(),
        ".loom holding non-spool content should survive"
    );
    assert!(loom_dir.exists(), "non-empty .loom dir should be kept");
}

#[cfg(unix)]
#[test]
fn test_remove_worktree_scaffold_does_not_follow_spool_symlink() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    let loom_dir = worktree_path.join(".loom");
    fs::create_dir_all(&loom_dir).unwrap();
    let outside_target = temp_dir.path().join("outside.jsonl");
    fs::write(&outside_target, "keep").unwrap();
    std::os::unix::fs::symlink(&outside_target, loom_dir.join("memory-spool.jsonl")).unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(
        fs::symlink_metadata(loom_dir.join("memory-spool.jsonl")).is_ok(),
        "a symlink at the spool path must never be followed or removed"
    );
    assert!(outside_target.exists(), "symlink target must be untouched");
}

#[test]
#[serial]
fn test_cleanup_worktree_succeeds_with_drained_spool_present() {
    // Regression test for the real failure this fixes: a stage that recorded
    // a memory note leaves `.loom/memory-spool.jsonl` on disk after the
    // daemon drains it, which made non-forced `git worktree remove` refuse
    // (untracked, and previously nothing removed or ignored it). This must
    // fail against the pre-fix code.
    let temp_dir = setup_git_repo();

    let worktrees_dir = temp_dir.path().join(".worktrees");
    fs::create_dir_all(&worktrees_dir).unwrap();
    git_ok(
        temp_dir.path(),
        &[
            "worktree",
            "add",
            ".worktrees/stage-1",
            "-b",
            "loom/stage-1",
        ],
    );

    let loom_dir = worktrees_dir.join("stage-1").join(".loom");
    fs::create_dir_all(&loom_dir).unwrap();
    fs::write(loom_dir.join("memory-spool.jsonl"), "{}\n").unwrap();

    let result = cleanup_worktree("stage-1", temp_dir.path(), false);
    assert!(result.is_ok(), "cleanup failed: {:?}", result.err());
    assert!(result.unwrap());
    assert!(!worktrees_dir.join("stage-1").exists());
}
