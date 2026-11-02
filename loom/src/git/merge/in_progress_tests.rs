//! In-progress merge detection in the main repository and in stage worktrees.

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

/// A forged `.git` file cannot choose the git directory or its configuration:
/// a `MERGE_HEAD` beside the foreign directory is no merge here, and with one
/// in the registered directory the listing still runs the foreign filter
/// never. The positive control runs last: git that follows the file runs it.
#[test]
fn a_worktree_merge_is_read_from_the_registered_git_directory() {
    use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    let marker = plant_foreign_git_dir(&worktree);
    let head = crate::git::runner::run_git_checked(&["rev-parse", "HEAD"], &repo).unwrap();
    std::fs::write(tmp.path().join("foreign.git/MERGE_HEAD"), &head).unwrap();

    assert_eq!(
        detect_in_progress_merge_at_worktree(&worktree).unwrap(),
        None
    );

    let admin = repo.join(".git/worktrees/s1").canonicalize().unwrap();
    std::fs::write(admin.join("MERGE_HEAD"), format!("{head}\n")).unwrap();
    let merge = detect_in_progress_merge_at_worktree(&worktree)
        .unwrap()
        .unwrap();

    assert!(!marker.exists(), "the listing ran the foreign filter");
    assert_eq!(merge.merge_heads, [head]);
    match merge.location {
        MergeLocation::Worktree { git_dir, .. } => assert_eq!(git_dir, admin),
        other => panic!("expected a worktree location, got {other:?}"),
    }

    let followed = crate::git::runner::run_git(&UNMERGED_PATHS_ARGS, &worktree).unwrap();
    assert!(followed.status.success(), "{followed:?}");
    assert!(marker.exists(), "the fixture's filter never runs");
}

#[test]
fn a_directory_the_repository_does_not_register_is_an_error() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    crate::verify::contracts::test_support::contract_worktree(&repo, "s1");
    let stranger = repo.join(".worktrees/stranger");
    std::fs::create_dir_all(&stranger).unwrap();

    let error = detect_in_progress_merge_at_worktree(&stranger).unwrap_err();

    assert!(
        format!("{error:#}").contains("not a registered worktree"),
        "{error:#}"
    );
}
