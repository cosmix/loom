//! A merged stage whose worktree HEAD is detached on commits no branch names: `loom clean`
//! must list that worktree and refuse without confirmation, since the forced removal deletes
//! the only place the commits are reachable from.

use std::io::IsTerminal;
use std::path::Path;

use serial_test::serial;

use super::super::worktrees::confirm_branch_deletion;
use super::*;
use crate::test_git::{git, TestRepo};

/// A repo whose stage `s` is merged into `main`, with its worktree's HEAD detached.
fn merged_stage_with_detached_worktree() -> TestRepo {
    let repo = TestRepo::builder()
        .seed("README.md", "base\n", "base")
        .create();
    let root = repo.path();
    let worktree = root.join(".worktrees").join("s");
    let worktree_arg = worktree.to_string_lossy().to_string();
    git(
        root,
        &["worktree", "add", "-b", "loom/s", &worktree_arg, "main"],
    );
    git(&worktree, &["checkout", "--detach"]);
    repo
}

fn commit_in_worktree(root: &Path) -> String {
    let worktree = root.join(".worktrees").join("s");
    std::fs::write(worktree.join("later.txt"), "later\n").unwrap();
    git(&worktree, &["add", "later.txt"]);
    git(&worktree, &["commit", "-m", "later"]);
    git(&worktree, &["rev-parse", "HEAD"])
}

#[test]
fn a_detached_head_with_unique_commits_is_held() {
    let repo = merged_stage_with_detached_worktree();
    assert!(held_worktrees(repo.path(), "main", &[]).unwrap().is_empty());

    commit_in_worktree(repo.path());

    let held = held_worktrees(repo.path(), "main", &[]).unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].stage_id, "s");
    assert!(held[0].why.contains("HEAD"), "{}", held[0].why);
}

#[test]
fn a_stage_the_branch_listing_names_is_not_listed_twice() {
    let repo = merged_stage_with_detached_worktree();
    commit_in_worktree(repo.path());
    let unmerged = [("loom/s".to_owned(), 1)];
    assert!(held_worktrees(repo.path(), "main", &unmerged)
        .unwrap()
        .is_empty());
}

#[test]
#[serial]
fn clean_refuses_without_confirmation_and_the_detached_commit_survives() {
    // A terminal would prompt; the refusal under test is the non-interactive one.
    if std::io::stdin().is_terminal() {
        return;
    }
    std::env::remove_var("LOOM_CLEAN_YES");
    let repo = merged_stage_with_detached_worktree();
    let commit = commit_in_worktree(repo.path());

    let proceed = confirm_branch_deletion(repo.path()).unwrap();

    assert!(!proceed, "clean would force-remove the unmerged worktree");
    git(repo.path(), &["cat-file", "-e", &commit]);
    assert!(repo.path().join(".worktrees/s/later.txt").exists());
}

#[test]
#[serial]
fn a_merged_clean_worktree_needs_no_confirmation() {
    std::env::remove_var("LOOM_CLEAN_YES");
    let repo = merged_stage_with_detached_worktree();
    assert!(confirm_branch_deletion(repo.path()).unwrap());
}
