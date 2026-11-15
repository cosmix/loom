use super::test_support::init_repo;
use super::*;
use crate::test_git::{commit_file, git, git_unchecked};

#[test]
fn test_parse_merge_stats() {
    let output = " 3 files changed, 10 insertions(+), 5 deletions(-)\n";
    let (files, ins, del) = parse_merge_stats(output);
    assert_eq!(files, 3);
    assert_eq!(ins, 10);
    assert_eq!(del, 5);
}

#[test]
fn test_parse_merge_stats_single_file() {
    let output = " 1 file changed, 2 insertions(+)\n";
    let (files, ins, del) = parse_merge_stats(output);
    assert_eq!(files, 1);
    assert_eq!(ins, 2);
    assert_eq!(del, 0);
}

fn start_conflicting_merge(root: &Path, branch: &str, branch_text: &str, main_text: &str) {
    git(root, &["checkout", "-b", branch]);
    commit_file(root, "a.txt", branch_text, "branch");
    git(root, &["checkout", "main"]);
    commit_file(root, "a.txt", main_text, "main");
    let merge = git_unchecked(root, &["merge", "--no-ff", branch]);
    assert!(
        merge_head_exists(root).unwrap(),
        "MERGE_HEAD missing; stdout={}, stderr={}",
        String::from_utf8_lossy(&merge.stdout),
        String::from_utf8_lossy(&merge.stderr),
    );
}

fn blocked_marker(result: Result<MergeResult>) -> String {
    match result.unwrap() {
        MergeResult::Blocked(MergeBlock::OperatorOperation { marker }) => marker,
        other => panic!("expected OperatorOperation, got {other:?}"),
    }
}

#[test]
fn merge_stage_refuses_when_merge_head_set() {
    let temp = init_repo();
    let root = temp.path();
    start_conflicting_merge(root, "loom/blockee", "branch", "main");
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let marker = blocked_marker(merge_stage(
        "blockee",
        "main",
        root,
        &work_dir,
        MergeGate::Enforce,
    ));
    assert_eq!(marker, "MERGE_HEAD");
    assert!(merge_head_exists(root).unwrap());
}

#[test]
fn merge_stage_refuses_during_cherry_pick() {
    let temp = init_repo();
    let root = temp.path();
    git(root, &["checkout", "-b", "side"]);
    commit_file(root, "a.txt", "side", "side");
    git(root, &["checkout", "main"]);
    commit_file(root, "a.txt", "main", "main");
    let pick = git_unchecked(root, &["cherry-pick", "side"]);
    assert!(!pick.status.success(), "cherry-pick must conflict");
    let work = super::test_support::lock_dir();

    let marker = blocked_marker(merge_stage(
        "blockee",
        "main",
        root,
        work.path(),
        MergeGate::Enforce,
    ));
    assert_eq!(marker, "CHERRY_PICK_HEAD");
    assert!(root.join(".git").join("CHERRY_PICK_HEAD").exists());
}

#[test]
fn merge_stage_refuses_during_rebase() {
    let temp = init_repo();
    let root = temp.path();
    std::fs::create_dir(root.join(".git").join("rebase-merge")).unwrap();
    let work = super::test_support::lock_dir();

    let marker = blocked_marker(merge_stage(
        "blockee",
        "main",
        root,
        work.path(),
        MergeGate::Enforce,
    ));
    assert_eq!(marker, "rebase-merge");
}
