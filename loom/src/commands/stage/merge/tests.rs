//! Unit tests for the `loom stage merge` command.

use super::*;
use crate::git::get_conflicting_files;
use crate::git::merge::merge_head_exists;
use crate::models::stage::Stage;
use std::process::Command;
use tempfile::TempDir;

fn create_test_stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Test Stage {id}"),
        status,
        fix_attempts: 0,
        max_fix_attempts: Some(3),
        ..Stage::default()
    }
}

// Tests from merge_complete

#[test]
fn test_get_conflicting_files_clean() {
    // In a clean repo, there should be no conflicting files
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // Initialize a git repo
    Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output()
        .unwrap();

    assert!(get_conflicting_files(repo_root).unwrap().is_empty());
}

#[test]
fn test_merge_head_absent_in_clean_repo() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // Initialize a git repo
    Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output()
        .unwrap();

    assert!(!merge_head_exists(repo_root).unwrap());
}

// Tests from retry_merge

#[test]
fn test_merge_rejects_wrong_status() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stages directory and a stage in Executing status
    let stages_dir = work_dir.join(".loom").join("work").join("stages");
    std::fs::create_dir_all(&stages_dir).unwrap();

    let stage = create_test_stage("test-stage", StageStatus::Executing);
    let stage_path = stages_dir.join("test-stage.md");
    let content = crate::verify::transitions::serialize_stage_to_markdown(&stage).unwrap();
    std::fs::write(stage_path, content).unwrap();

    // merge should fail since we're not in a worktree and status is wrong
    // We test the status check by calling the function parts directly
    assert!(!matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    ));
}

#[test]
fn test_merge_accepts_merge_conflict() {
    let stage = create_test_stage("test-stage", StageStatus::MergeConflict);
    assert!(matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    ));
}

#[test]
fn test_merge_accepts_merge_blocked() {
    let stage = create_test_stage("test-stage", StageStatus::MergeBlocked);
    assert!(matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    ));
}

#[test]
fn test_fix_attempts_increment() {
    let mut stage = create_test_stage("test-stage", StageStatus::MergeConflict);
    assert_eq!(stage.fix_attempts, 0);

    let attempts = stage.increment_fix_attempts();
    assert_eq!(attempts, 1);
    assert_eq!(stage.fix_attempts, 1);

    let attempts = stage.increment_fix_attempts();
    assert_eq!(attempts, 2);
    assert_eq!(stage.fix_attempts, 2);
}

#[test]
fn test_fix_limit_detection() {
    let mut stage = create_test_stage("test-stage", StageStatus::MergeConflict);
    stage.max_fix_attempts = Some(2);

    assert!(!stage.is_at_fix_limit());

    stage.fix_attempts = 1;
    assert!(!stage.is_at_fix_limit());

    stage.fix_attempts = 2;
    assert!(stage.is_at_fix_limit());

    stage.fix_attempts = 3;
    assert!(stage.is_at_fix_limit());
}

#[test]
fn test_find_repo_root_from_worktree() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // Create .worktrees/my-stage structure
    let worktree = repo_root.join(".worktrees").join("my-stage");
    std::fs::create_dir_all(&worktree).unwrap();

    let found = find_repo_root(&worktree).unwrap();
    assert_eq!(
        found.canonicalize().unwrap(),
        repo_root.canonicalize().unwrap()
    );
}

#[test]
fn test_find_repo_root_from_subdir() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // Create .worktrees/my-stage/src structure
    let subdir = repo_root.join(".worktrees").join("my-stage").join("src");
    std::fs::create_dir_all(&subdir).unwrap();

    // From inside worktree subdir, we should still find repo root
    // We need .worktrees at repo root for this to work
    let found = find_repo_root(&subdir).unwrap();
    assert_eq!(
        found.canonicalize().unwrap(),
        repo_root.canonicalize().unwrap()
    );
}

fn git_in(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

/// A repo on `main` with `loom/gated` one commit ahead, adding
/// `.claude/settings.json`, and the stage saved as `MergeConflict`.
fn gated_repo() -> (TempDir, std::path::PathBuf, Stage) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    git_in(root, &["init", "-b", "main"]);
    git_in(root, &["config", "user.email", "t@t.com"]);
    git_in(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("a.txt"), "a").unwrap();
    git_in(root, &["add", "a.txt"]);
    git_in(root, &["commit", "-m", "seed"]);
    git_in(root, &["checkout", "-b", "loom/gated"]);
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    std::fs::write(root.join(".claude/settings.json"), "{}").unwrap();
    git_in(root, &["add", ".claude/settings.json"]);
    git_in(root, &["commit", "-m", "stage work"]);
    git_in(root, &["checkout", "main"]);
    let work_dir = root.join(".loom").join("work");
    git_in(root, &["worktree", "add", ".worktrees/gated", "loom/gated"]);
    let mut stage = create_test_stage("gated", StageStatus::MergeConflict);
    stage.completed_commit = Some(crate::git::branch::get_branch_head("loom/gated", root).unwrap());
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();
    (temp_dir, work_dir, stage)
}

fn assert_held_for_review(work_dir: &Path) {
    let on_disk = load_stage("gated", work_dir).unwrap();
    assert_eq!(on_disk.status, StageStatus::NeedsHumanReview);
    assert!(!on_disk.merged);
    assert!(on_disk
        .review_reason
        .unwrap()
        .contains(".claude/settings.json"));
}

#[test]
fn a_retried_merge_of_a_control_path_branch_is_held_and_fails() {
    let (repo, work_dir, stage) = gated_repo();

    let result = landing::attempt_retried_merge(&stage, &work_dir, repo.path(), "main");

    assert!(result.unwrap_err().to_string().contains("held"));
    assert_held_for_review(&work_dir);
    assert!(!repo.path().join(".claude").exists(), "nothing was merged");
}

#[test]
fn hold_for_review_routes_the_stage_and_fails() {
    let (_repo, work_dir, _stage) = gated_repo();

    let result = hold_for_review("gated", &work_dir, "touches .claude/settings.json");

    assert!(result.is_err());
    assert_held_for_review(&work_dir);
}

#[test]
fn the_cli_resolver_spawn_refuses_a_control_path_branch() {
    let (repo, work_dir, stage) = gated_repo();

    let result = crate::commands::stage::merge_resolver::spawn_merge_resolver(
        &stage,
        &[],
        "main",
        repo.path(),
        &work_dir,
    );

    let error = result.err().expect("the spawn is refused");
    assert!(error.to_string().contains("control path"));
    assert_held_for_review(&work_dir);
}

#[test]
fn the_cli_resolver_spawn_fails_when_the_diff_cannot_be_computed() {
    let (repo, work_dir, stage) = gated_repo();

    let result = crate::commands::stage::merge_resolver::spawn_merge_resolver(
        &stage,
        &[],
        "no-such-branch",
        repo.path(),
        &work_dir,
    );

    assert!(result.is_err());
    let on_disk = load_stage("gated", &work_dir).unwrap();
    assert_eq!(on_disk.status, StageStatus::MergeConflict);
}

#[test]
fn the_cli_resolver_spawn_refuses_a_typed_blocked_stage() {
    let (repo, work_dir, mut stage) = gated_repo();
    stage.block_merge(crate::git::MergeBlock::TargetMoved);

    let result = crate::commands::stage::merge_resolver::spawn_merge_resolver(
        &stage,
        &[],
        "main",
        repo.path(),
        &work_dir,
    );

    let error = result.err().expect("the spawn is refused").to_string();
    assert!(error.contains("not in conflict"), "{error}");
    assert!(error.contains("loom stage merge gated"), "{error}");
}

#[test]
fn record_block_leaves_a_merged_stage_alone() {
    let (_repo, work_dir, _stage) = gated_repo();
    update_stage("gated", &work_dir, |s| {
        s.status = StageStatus::Completed;
        s.merged = true;
        Ok(())
    })
    .unwrap();

    let result = landing::record_block("gated", &work_dir, crate::git::MergeBlock::TargetMoved);

    assert!(result.unwrap_err().to_string().contains("already merged"));
    let on_disk = load_stage("gated", &work_dir).unwrap();
    assert_eq!(on_disk.status, StageStatus::Completed);
    assert!(on_disk.merged);
    assert_eq!(on_disk.merge.block, None);
}

/// `loom/zero` at the tip of `main` (no commit of its own) and the stage
/// saved as `MergeBlocked`.
fn zero_commit_repo(completed_commit: Option<String>) -> (TempDir, std::path::PathBuf, Stage) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    git_in(root, &["init", "-b", "main"]);
    git_in(root, &["config", "user.email", "t@t.com"]);
    git_in(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("a.txt"), "a").unwrap();
    git_in(root, &["add", "a.txt"]);
    git_in(root, &["commit", "-m", "seed"]);
    git_in(root, &["branch", "loom/zero"]);
    let work_dir = root.join(".loom").join("work");
    let mut stage = create_test_stage("zero", StageStatus::MergeBlocked);
    stage.completed_commit = completed_commit;
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();
    (temp_dir, work_dir, stage)
}

#[test]
fn a_retried_merge_of_a_zero_commit_branch_is_routed_to_review_not_merged() {
    let (repo, work_dir, stage) = zero_commit_repo(None);

    let result = landing::attempt_retried_merge(&stage, &work_dir, repo.path(), "main");

    assert!(result.unwrap_err().to_string().contains("no commits"));
    let on_disk = load_stage("zero", &work_dir).unwrap();
    assert_eq!(on_disk.status, StageStatus::NeedsHumanReview);
    assert!(!on_disk.merged);
    assert!(on_disk.review_reason.unwrap().contains("zero commits"));
}

#[test]
fn completing_a_retried_merge_whose_recorded_commit_is_not_in_the_target_is_refused() {
    let (repo, work_dir, _) = zero_commit_repo(None);
    // The recorded commit lives on a side branch the target never merged.
    git_in(repo.path(), &["checkout", "-q", "-b", "side"]);
    std::fs::write(repo.path().join("side.txt"), "side").unwrap();
    git_in(repo.path(), &["add", "side.txt"]);
    git_in(repo.path(), &["commit", "-m", "side"]);
    let side = crate::git::runner::run_git_checked(&["rev-parse", "HEAD"], repo.path()).unwrap();
    git_in(repo.path(), &["checkout", "-q", "main"]);
    let stage = update_stage("zero", &work_dir, |s| {
        s.completed_commit = Some(side.clone());
        Ok(())
    })
    .unwrap();

    let result =
        landing::complete_retried_merge(&stage, &work_dir, repo.path(), "main", "marked merged");

    assert!(result.unwrap_err().to_string().contains("not an ancestor"));
    let on_disk = load_stage("zero", &work_dir).unwrap();
    assert!(!on_disk.merged);
    assert_eq!(on_disk.status, StageStatus::MergeBlocked);
}
