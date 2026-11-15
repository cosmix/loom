//! A tag named like a branch must not answer a question about the branch:
//! tags are shared by every worktree, and git resolves a bare name to the tag
//! first.

use super::test_support::{init_repo, stage_branch};
use super::*;
use crate::git::branch::{commits_ahead_of, get_branch_head};
use crate::test_git::git;

fn head_ref(root: &Path, name: &str) -> String {
    git(root, &["rev-parse", &format!("refs/heads/{name}")])
}

#[test]
fn a_tag_named_like_the_target_cannot_fake_a_merge() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "work")]);
    let tip = head_ref(root, "loom/s1");
    git(root, &["tag", "main", &tip]);

    assert!(!verify_merge_succeeded(&tip, "main", root).unwrap());
    assert_eq!(commits_ahead_of("loom/s1", "main", root).unwrap(), 1);
}

#[test]
fn a_tag_named_like_the_branch_does_not_change_its_head() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "work")]);
    let tip = head_ref(root, "loom/s1");
    git(root, &["tag", "loom/s1", &head_ref(root, "main")]);

    assert_eq!(get_branch_head("loom/s1", root).unwrap(), tip);
}

#[test]
fn a_full_ref_is_passed_through() {
    let repo = init_repo();
    let root = repo.path();
    let head = head_ref(root, "main");

    assert!(verify_merge_succeeded(&head, "refs/heads/main", root).unwrap());
}

#[test]
fn a_tag_named_like_the_target_does_not_stop_the_merge() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "work")]);
    let tip = head_ref(root, "loom/s1");
    git(root, &["tag", "main", &tip]);
    let work = super::test_support::lock_dir();

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap();

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
}
