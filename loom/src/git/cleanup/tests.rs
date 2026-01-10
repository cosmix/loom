//! Tests for cleanup operations

use crate::git::cleanup::worktree::remove_worktree_symlinks;
use crate::git::cleanup::{
    cleanup_after_merge, cleanup_branch, cleanup_worktree, needs_cleanup, prune_worktrees,
    CleanupConfig, CleanupResult,
};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn setup_git_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    // Create initial commit
    let test_file = temp_dir.path().join("README.md");
    fs::write(&test_file, "# Test").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    temp_dir
}

#[test]
fn test_cleanup_config_default() {
    let config = CleanupConfig::default();
    assert!(config.force_worktree_removal);
    assert!(!config.force_branch_deletion);
    assert!(config.prune_worktrees);
    assert!(config.verbose);
}

#[test]
fn test_cleanup_config_quiet() {
    let config = CleanupConfig::quiet();
    assert!(!config.verbose);
}

#[test]
fn test_cleanup_config_forced() {
    let config = CleanupConfig::forced();
    assert!(config.force_worktree_removal);
    assert!(config.force_branch_deletion);
}

#[test]
fn test_cleanup_result_is_complete() {
    let mut result = CleanupResult::default();
    assert!(!result.is_complete());

    result.worktree_removed = true;
    result.branch_deleted = true;
    assert!(result.is_complete());

    result.warnings.push("warning".to_string());
    assert!(!result.is_complete());
}

#[test]
fn test_cleanup_result_any_cleanup_done() {
    let mut result = CleanupResult::default();
    assert!(!result.any_cleanup_done());

    result.worktree_removed = true;
    assert!(result.any_cleanup_done());
}

#[test]
fn test_cleanup_worktree_nonexistent() {
    let temp_dir = setup_git_repo();
    let result = cleanup_worktree("nonexistent", temp_dir.path(), false);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_cleanup_branch_nonexistent() {
    let temp_dir = setup_git_repo();
    let result = cleanup_branch("nonexistent", temp_dir.path(), false);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_needs_cleanup_no_resources() {
    let temp_dir = setup_git_repo();
    assert!(!needs_cleanup("stage-1", temp_dir.path()));
}

#[test]
fn test_needs_cleanup_with_worktree_dir() {
    let temp_dir = setup_git_repo();
    let worktree_path = temp_dir.path().join(".worktrees").join("stage-1");
    fs::create_dir_all(&worktree_path).unwrap();

    assert!(needs_cleanup("stage-1", temp_dir.path()));
}

#[test]
fn test_needs_cleanup_with_branch() {
    let temp_dir = setup_git_repo();

    // Create a branch
    Command::new("git")
        .args(["branch", "loom/stage-1"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    assert!(needs_cleanup("stage-1", temp_dir.path()));
}

#[test]
fn test_prune_worktrees() {
    let temp_dir = setup_git_repo();
    let result = prune_worktrees(temp_dir.path());
    assert!(result.is_ok());
}

#[test]
fn test_cleanup_after_merge_nothing_to_clean() {
    let temp_dir = setup_git_repo();
    let config = CleanupConfig::quiet();

    let result = cleanup_after_merge("nonexistent", temp_dir.path(), &config);
    assert!(result.is_ok());

    let cleanup_result = result.unwrap();
    assert!(!cleanup_result.worktree_removed);
    assert!(!cleanup_result.branch_deleted);
}

#[test]
fn test_cleanup_multiple_stages_empty() {
    use crate::git::cleanup::cleanup_multiple_stages;

    let temp_dir = setup_git_repo();
    let config = CleanupConfig::quiet();

    let results = cleanup_multiple_stages(&[], temp_dir.path(), &config);
    assert!(results.is_empty());
}

#[test]
fn test_remove_worktree_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    fs::create_dir_all(&worktree_path).unwrap();

    // Create .claude directory with symlinks (simulated as files for testing)
    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("CLAUDE.md"), "test").unwrap();
    fs::write(claude_dir.join("settings.local.json"), "{}").unwrap();

    let result = remove_worktree_symlinks(&worktree_path);
    assert!(result.is_ok());
}
