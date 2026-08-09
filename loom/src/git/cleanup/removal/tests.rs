use super::*;
use crate::git::cleanup::cleanup_after_merge;
use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
        .env("GIT_CONFIG_SYSTEM", root.join(".no-system-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "test@example.com"]);
    git_ok(root, &["config", "user.name", "Test"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git_ok(root, &["add", "README.md"]);
    git_ok(root, &["commit", "-m", "base"]);
    temp
}

fn create_stage_worktree(root: &Path, stage_id: &str) -> (PathBuf, String) {
    let worktrees = root.join(".worktrees");
    fs::create_dir_all(&worktrees).unwrap();
    let worktree = worktrees.join(stage_id);
    let worktree_arg = worktree.to_string_lossy().to_string();
    let branch = branch_name_for_stage(stage_id);
    git_ok(
        root,
        &["worktree", "add", "-b", &branch, &worktree_arg, "main"],
    );
    fs::write(worktree.join("feature.txt"), "feature\n").unwrap();
    git_ok(&worktree, &["add", "feature.txt"]);
    git_ok(&worktree, &["commit", "-m", "feature"]);
    let commit = git_stdout(&worktree, &["rev-parse", "HEAD"]);
    (worktree, commit)
}

fn merge_stage(root: &Path, stage_id: &str) {
    let branch = branch_name_for_stage(stage_id);
    git_ok(root, &["merge", "--no-ff", "-m", "merge stage", &branch]);
}

#[test]
fn verified_cleanup_refuses_modified_worktree() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "dirty");
    merge_stage(root, "dirty");
    fs::write(worktree.join("feature.txt"), "uncommitted\n").unwrap();

    let error = cleanup_verified_stage("dirty", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("uncommitted changes"));
    assert!(worktree.join("feature.txt").exists());
    assert!(branch_exists_strict("loom/dirty", root).unwrap());
}

#[test]
fn verified_cleanup_refuses_untracked_worktree_file() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "untracked");
    merge_stage(root, "untracked");
    fs::write(worktree.join("notes.txt"), "keep me\n").unwrap();

    let error = cleanup_verified_stage("untracked", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("notes.txt"));
    assert!(worktree.join("notes.txt").exists());
}

#[test]
fn verified_cleanup_refuses_unmerged_commit() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "unmerged");

    let error = cleanup_verified_stage("unmerged", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("not retained"));
    assert!(worktree.exists());
    assert!(branch_exists_strict("loom/unmerged", root).unwrap());
}

#[test]
fn verified_cleanup_refuses_unmerged_stage_base_branch() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "diverged-base");
    merge_stage(root, "diverged-base");
    git_ok(root, &["checkout", "-b", "loom/_base/diverged-base"]);
    fs::write(root.join("base-only.txt"), "not retained\n").unwrap();
    git_ok(root, &["add", "base-only.txt"]);
    git_ok(root, &["commit", "-m", "unmerged base change"]);
    git_ok(root, &["checkout", "main"]);

    let error = cleanup_verified_stage("diverged-base", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("stage base branch head"));
    assert!(worktree.exists());
    assert!(branch_exists_strict("loom/diverged-base", root).unwrap());
    assert!(base_branch_exists("diverged-base", root).unwrap());
}

#[test]
fn verified_cleanup_refuses_when_both_resources_are_absent() {
    let temp = init_repo();
    let root = temp.path();
    let main_commit = git_stdout(root, &["rev-parse", "main"]);

    let error = cleanup_verified_stage("absent", &main_commit, "main", root).unwrap_err();
    assert!(error
        .to_string()
        .contains("refusing to infer that it was merged"));
}

#[test]
fn cleanup_failure_is_returned_as_an_error() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".worktrees/broken")).unwrap();
    let config = CleanupConfig {
        force_worktree_removal: false,
        force_branch_deletion: false,
        prune_worktrees: true,
        verbose: false,
    };

    let error = cleanup_after_merge("broken", root, &config).unwrap_err();
    assert!(error.to_string().contains("Failed to remove worktree"));
    assert!(root.join(".worktrees/broken").exists());
}

#[test]
fn verified_cleanup_removes_legitimately_merged_resources() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "merged");
    merge_stage(root, "merged");

    let result = cleanup_verified_stage("merged", &commit, "main", root).unwrap();
    assert!(result.worktree_removed);
    assert!(result.branch_deleted);
    assert!(!worktree.exists());
    assert!(!branch_exists_strict("loom/merged", root).unwrap());
}

#[test]
fn destructive_cleanup_requires_exact_confirmation() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage_worktree(root, "forced");

    let error = cleanup_destructive_stage("forced", "forced", root).unwrap_err();
    assert!(error.to_string().contains("exact confirmation"));
    assert!(worktree.exists());

    let confirmation = destructive_removal_confirmation("forced");
    let result = cleanup_destructive_stage("forced", &confirmation, root).unwrap();
    assert!(result.worktree_removed);
    assert!(result.branch_deleted);
}

#[test]
fn destructive_cleanup_rejects_stage_id_path_traversal() {
    let temp = init_repo();
    let root = temp.path();
    let confirmation = destructive_removal_confirmation("../outside");

    let error = cleanup_destructive_stage("../outside", &confirmation, root).unwrap_err();
    assert!(error.to_string().contains("Invalid stage ID"));
}
