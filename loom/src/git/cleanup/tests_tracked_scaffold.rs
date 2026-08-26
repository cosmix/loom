//! Tests for the interaction between git-tracked `.claude`/`CLAUDE.md`
//! content and scaffold removal, plus the base-branch visibility fixes to
//! `needs_cleanup` / `stage_resources_exist`.
//!
//! The property under test: removal never destroys anything git tracks —
//! creation only plants scaffold when the checkout carries none of its own,
//! so a tracked `.claude/` entry or root `CLAUDE.md` is the repo's own file,
//! not loom's scaffold, and must survive both `remove_worktree_scaffold` and
//! the non-forced `git worktree remove` it precedes.

use super::tests::{git_ok, setup_git_repo};
use crate::git::cleanup::worktree::remove_worktree_scaffold;
use crate::git::cleanup::{cleanup_worktree, needs_cleanup, stage_resources_exist};
use serial_test::serial;
use std::fs;

#[test]
#[serial]
fn tracked_claude_settings_survive_scaffold_removal_and_cleanup_succeeds() {
    let temp_dir = setup_git_repo();
    let claude_dir = temp_dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("settings.json"), "{}").unwrap();
    git_ok(temp_dir.path(), &["add", ".claude/settings.json"]);
    git_ok(temp_dir.path(), &["commit", "-m", "track claude settings"]);

    fs::create_dir_all(temp_dir.path().join(".worktrees")).unwrap();
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

    let result = cleanup_worktree("stage-1", temp_dir.path(), false);
    assert!(result.is_ok(), "cleanup failed: {:?}", result.err());
    assert!(result.unwrap());
    assert!(!temp_dir.path().join(".worktrees/stage-1").exists());
}

#[cfg(unix)]
#[test]
#[serial]
fn tracked_root_claude_md_symlink_survives_scaffold_removal() {
    let temp_dir = setup_git_repo();
    fs::create_dir_all(temp_dir.path().join("docs")).unwrap();
    fs::write(temp_dir.path().join("docs/rules.md"), "project rules").unwrap();
    std::os::unix::fs::symlink("docs/rules.md", temp_dir.path().join("CLAUDE.md")).unwrap();
    git_ok(temp_dir.path(), &["add", "docs/rules.md", "CLAUDE.md"]);
    git_ok(
        temp_dir.path(),
        &["commit", "-m", "track CLAUDE.md symlink"],
    );

    fs::create_dir_all(temp_dir.path().join(".worktrees")).unwrap();
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

    let worktree = temp_dir.path().join(".worktrees/stage-1");
    let result = remove_worktree_scaffold(&worktree);
    assert!(
        result.is_ok(),
        "scaffold removal failed: {:?}",
        result.err()
    );
    assert!(
        fs::symlink_metadata(worktree.join("CLAUDE.md")).is_ok(),
        "tracked CLAUDE.md symlink should survive scaffold removal"
    );
}

#[test]
#[serial]
fn blocking_paths_are_capped() {
    let temp_dir = setup_git_repo();
    fs::create_dir_all(temp_dir.path().join(".worktrees")).unwrap();
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

    let worktree = temp_dir.path().join(".worktrees/stage-1");
    for i in 0..25 {
        fs::write(worktree.join(format!("scratch-{i:02}.txt")), "x").unwrap();
    }

    let result = cleanup_worktree("stage-1", temp_dir.path(), false);
    assert!(result.is_err());
    let message = format!("{:#}", result.unwrap_err());
    assert!(
        message.contains("scratch-00.txt"),
        "expected the first blocking path, got: {message}"
    );
    assert!(
        message.contains("\u{2026} and 5 more"),
        "expected a capped summary line, got: {message}"
    );
    assert!(
        !message.contains("scratch-24.txt"),
        "expected the 25th path to be dropped from the message, got: {message}"
    );
}

#[test]
fn needs_cleanup_sees_a_lone_base_branch() {
    let temp_dir = setup_git_repo();
    git_ok(temp_dir.path(), &["branch", "loom/_base/stage-1"]);
    assert!(needs_cleanup("stage-1", temp_dir.path()));
}

#[test]
fn stage_resources_exist_is_false_when_nothing_remains() {
    let temp_dir = setup_git_repo();
    assert!(!stage_resources_exist("stage-1", temp_dir.path()).unwrap());
}

#[test]
fn stage_resources_exist_is_true_for_a_lone_base_branch() {
    let temp_dir = setup_git_repo();
    git_ok(temp_dir.path(), &["branch", "loom/_base/stage-1"]);
    assert!(stage_resources_exist("stage-1", temp_dir.path()).unwrap());
}
