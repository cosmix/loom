//! `accepted_change_base` against scratch repositories: where the change a
//! stage merge brought to the target starts.

use super::super::merge_stage;
use super::super::test_support::{
    commit_file, git_ok, git_out, init_repo, lock_dir, rev, stage_branch,
};
use super::*;

fn merge(root: &Path, id: &str) {
    let lock = lock_dir();
    merge_stage(id, "main", root, lock.path(), MergeGate::Enforce).unwrap();
}

fn sorted_changes(root: &Path, base: &str, completed: &str) -> Vec<String> {
    let mut paths = changed_paths(root, base, completed).unwrap();
    paths.sort();
    paths
}

#[test]
fn a_merged_stage_starts_at_its_fork_point() {
    let temp = init_repo();
    let root = temp.path();
    stage_branch(root, "s", &[("s1.txt", "a"), ("s2.txt", "b")]);
    let completed = rev(root, "loom/s");
    let fork = rev(root, "main");
    merge(root, "s");
    let AcceptedBase::Merged { merge, base } =
        accepted_change_base(root, &completed, "main").unwrap()
    else {
        panic!("expected a merged stage");
    };
    assert_eq!(merge, rev(root, "main"));
    assert_eq!(base, fork);
    assert_eq!(
        sorted_changes(root, &base, &completed),
        ["s1.txt", "s2.txt"]
    );
}

#[test]
fn a_target_merged_into_the_stage_branch_is_not_its_change() {
    let temp = init_repo();
    let root = temp.path();
    stage_branch(root, "s", &[("s1.txt", "a")]);
    commit_file(root, "other.txt", "o", "other");
    let x = rev(root, "main");
    git_ok(root, &["checkout", "loom/s"]);
    git_ok(root, &["merge", "--no-edit", "main"]);
    git_ok(root, &["checkout", "main"]);
    commit_file(root, "third.txt", "t", "third");
    let completed = rev(root, "loom/s");
    merge(root, "s");
    let AcceptedBase::Merged { base, .. } = accepted_change_base(root, &completed, "main").unwrap()
    else {
        panic!("expected a merged stage");
    };
    assert_eq!(base, x);
    assert_eq!(sorted_changes(root, &base, &completed), ["s1.txt"]);
}

#[test]
fn an_unmerged_stage_uses_the_merge_base_with_the_target() {
    let temp = init_repo();
    let root = temp.path();
    stage_branch(root, "s", &[("s1.txt", "a")]);
    commit_file(root, "other.txt", "o", "other");
    let completed = rev(root, "loom/s");
    let expected = git_out(root, &["merge-base", &completed, "main"]);
    assert_eq!(
        accepted_change_base(root, &completed, "main").unwrap(),
        AcceptedBase::Unmerged { base: expected }
    );
}

#[test]
fn a_value_that_is_not_a_commit_id_is_refused() {
    let temp = init_repo();
    let root = temp.path();
    assert!(accepted_change_base(root, "--all", "main").is_err());
    assert!(accepted_change_base(root, "abc", "main").is_err());
    git_ok(root, &["branch", &"g".repeat(40)]);
    // Values git itself accepts as revisions: only the commit-id guard refuses them.
    assert!(accepted_change_base(root, "HEAD", "main").is_err());
    assert!(accepted_change_base(root, "main", "main").is_err());
    assert!(accepted_change_base(root, &"g".repeat(40), "main").is_err());
}

#[test]
fn a_later_stage_merge_is_skipped() {
    let temp = init_repo();
    let root = temp.path();
    stage_branch(root, "s", &[("s1.txt", "a")]);
    let completed = rev(root, "loom/s");
    let fork = rev(root, "main");
    merge(root, "s");
    stage_branch(root, "t", &[("t1.txt", "c")]);
    merge(root, "t");
    let s_merge = git_out(root, &["rev-parse", "main^1"]);
    let AcceptedBase::Merged { merge, base } =
        accepted_change_base(root, &completed, "main").unwrap()
    else {
        panic!("expected a merged stage");
    };
    assert_eq!(merge, s_merge);
    assert_eq!(base, fork);
    assert_eq!(sorted_changes(root, &base, &completed), ["s1.txt"]);
}

#[test]
fn a_fast_forwarded_commit_has_no_merge() {
    let temp = init_repo();
    let root = temp.path();
    stage_branch(root, "s", &[("s1.txt", "a")]);
    let completed = rev(root, "loom/s");
    git_out(root, &["merge", "--ff-only", "loom/s"]);
    assert_eq!(
        accepted_change_base(root, &completed, "main").unwrap(),
        AcceptedBase::Unmerged { base: completed }
    );
}
