//! Tests for cleanup operations

use crate::git::cleanup::worktree::remove_worktree_scaffold;
use crate::git::cleanup::{
    cleanup_after_merge, cleanup_branch, cleanup_worktree, needs_cleanup, prune_worktrees,
    CleanupConfig, CleanupResult,
};
use serial_test::serial;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Run git with ambient configuration neutralized, so a developer's global
/// settings (hooks, gpg signing, default branch, aliases) cannot change what
/// these tests exercise. Mirrors
/// `orchestrator::merge_lifecycle::tests::isolated_git`.
fn isolated_git(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

/// Assert a setup command succeeded. Dropping the exit status here would hide
/// setup failures and turn them into a confusing assertion further down.
pub(super) fn git_ok(root: &Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

pub(super) fn setup_git_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    git_ok(temp_dir.path(), &["init"]);
    git_ok(temp_dir.path(), &["config", "user.email", "test@test.com"]);
    git_ok(temp_dir.path(), &["config", "user.name", "Test"]);

    // Create initial commit
    let test_file = temp_dir.path().join("README.md");
    fs::write(&test_file, "# Test").unwrap();
    git_ok(temp_dir.path(), &["add", "."]);
    git_ok(temp_dir.path(), &["commit", "-m", "Initial commit"]);

    temp_dir
}

#[test]
fn test_cleanup_config_default() {
    let config = CleanupConfig::default();
    assert!(!config.force_worktree_removal);
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

#[cfg(unix)]
#[test]
fn test_cleanup_worktree_rejects_symlink_path() {
    let temp_dir = setup_git_repo();
    let outside = temp_dir.path().join("user-data");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep").unwrap();
    let worktrees = temp_dir.path().join(".worktrees");
    fs::create_dir_all(&worktrees).unwrap();
    std::os::unix::fs::symlink(&outside, worktrees.join("linked")).unwrap();

    let result = cleanup_worktree("linked", temp_dir.path(), true);

    assert!(result.is_err());
    assert!(outside.join("keep.txt").exists());
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
fn test_remove_worktree_scaffold() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    fs::create_dir_all(&worktree_path).unwrap();

    // Create the generated regular settings file.
    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("settings.local.json"), "{}").unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(!claude_dir.exists());
}

#[test]
fn test_remove_worktree_scaffold_preserves_unknown_claude_content() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("notes.md"), "keep").unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(claude_dir.join("notes.md").exists());
    assert!(claude_dir.exists(), "non-empty .claude dir should be kept");
}

#[test]
fn test_remove_worktree_scaffold_preserves_regular_claude_instructions() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("CLAUDE.md"), "user instructions").unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(claude_dir.join("CLAUDE.md").exists());
}

#[test]
fn test_remove_worktree_scaffold_leaves_tracked_root_claude_md() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(worktree_path.join("CLAUDE.md"), "project rules").unwrap();
    fs::write(claude_dir.join("settings.local.json"), "{}").unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert_eq!(
        fs::read_to_string(worktree_path.join("CLAUDE.md")).unwrap(),
        "project rules"
    );
    assert!(
        !claude_dir.exists(),
        "generated .claude scaffold should be removed"
    );
}

#[cfg(unix)]
#[test]
fn test_remove_worktree_scaffold_removes_root_claude_md_symlink() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    fs::create_dir_all(&worktree_path).unwrap();
    let target = temp_dir.path().join("real-claude.md");
    fs::write(&target, "target content").unwrap();
    std::os::unix::fs::symlink(&target, worktree_path.join("CLAUDE.md")).unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(
        fs::symlink_metadata(worktree_path.join("CLAUDE.md")).is_err(),
        "symlinked root CLAUDE.md should be removed"
    );
}

#[test]
fn test_remove_worktree_scaffold_skips_runtime_dirs_in_claude() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join("worktree");
    let claude_dir = worktree_path.join(".claude");
    let runtime_dir = claude_dir.join(".cc-writes");
    fs::create_dir_all(&runtime_dir).unwrap();
    fs::write(claude_dir.join("settings.local.json"), "{}").unwrap();
    fs::write(runtime_dir.join("scratch.txt"), "runtime state").unwrap();

    let result = remove_worktree_scaffold(&worktree_path);
    assert!(result.is_ok());
    assert!(!claude_dir.join("settings.local.json").exists());
    assert!(
        runtime_dir.exists(),
        ".cc-writes runtime dir should be kept"
    );
    assert!(claude_dir.exists(), "non-empty .claude dir should be kept");
}

#[test]
#[serial]
fn test_cleanup_worktree_succeeds_when_repo_tracks_root_claude_md() {
    let temp_dir = setup_git_repo();
    fs::write(temp_dir.path().join("CLAUDE.md"), "tracked instructions").unwrap();
    git_ok(temp_dir.path(), &["add", "CLAUDE.md"]);
    git_ok(temp_dir.path(), &["commit", "-m", "add CLAUDE.md"]);

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

    let worktree_claude_md = worktrees_dir.join("stage-1").join("CLAUDE.md");
    assert!(worktree_claude_md.exists());
    assert!(!fs::symlink_metadata(&worktree_claude_md)
        .unwrap()
        .is_symlink());

    let result = cleanup_worktree("stage-1", temp_dir.path(), false);
    assert!(result.is_ok(), "cleanup failed: {:?}", result.err());
    assert!(result.unwrap());
    assert!(!worktrees_dir.join("stage-1").exists());
}

#[test]
#[serial]
fn test_cleanup_worktree_refusal_names_blocking_paths() {
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

    fs::write(
        worktrees_dir.join("stage-1").join("scratch.txt"),
        "untracked",
    )
    .unwrap();

    let result = cleanup_worktree("stage-1", temp_dir.path(), false);
    assert!(result.is_err());
    let message = format!("{:#}", result.unwrap_err());
    assert!(
        message.contains("scratch.txt"),
        "expected error to name the blocking path, got: {message}"
    );
    assert!(worktrees_dir.join("stage-1").exists());
}
