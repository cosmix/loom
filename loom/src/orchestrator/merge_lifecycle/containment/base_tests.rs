//! The daemon refuses cleanup while `loom/_base/<id>`, which cleanup deletes,
//! holds commits the target lacks.

use super::super::tests::{merge_stage_branch, repo_with_stage_commit};
use super::*;
use crate::test_git::git;

#[test]
fn a_base_branch_ahead_of_the_target_refuses_cleanup() {
    let (temp, _head) = repo_with_stage_commit("based");
    let root = temp.path();
    merge_stage_branch(root, "based");
    git(root, &["checkout", "-q", "-b", "loom/_base/based", "main"]);
    std::fs::write(root.join("base-only.txt"), "not retained\n").unwrap();
    git(root, &["add", "base-only.txt"]);
    git(root, &["commit", "-m", "unmerged base change"]);
    git(root, &["checkout", "-q", "main"]);
    let work_dir = root.join(".loom").join("work");

    let refusal = containment_refusal(&MergeLifecycle::new("based", root, &work_dir), "main")
        .expect("cleanup must be refused");

    assert!(refusal.contains("stage base branch head"), "{refusal}");
}
