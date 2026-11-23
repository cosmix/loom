//! Contract: the daemon's containment check looks at the worktree HEAD even
//! while the stage branch exists and is merged, so a detached HEAD holding
//! commits the target lacks refuses cleanup.

use super::super::tests::{merge_stage_branch, repo_with_stage_commit, worktree_of};
use super::*;
use crate::test_git::git;

#[test]
fn a_detached_head_with_unique_commits_refuses_daemon_cleanup() {
    let (temp, _head) = repo_with_stage_commit("s");
    let root = temp.path();
    merge_stage_branch(root, "s");
    let worktree = worktree_of(root, "s");
    git(&worktree, &["checkout", "--detach"]);
    std::fs::write(worktree.join("later.txt"), "later\n").unwrap();
    git(&worktree, &["add", "later.txt"]);
    git(&worktree, &["commit", "-m", "later"]);
    let work = tempfile::TempDir::new().unwrap();
    let lifecycle = MergeLifecycle::new("s", root, work.path());

    let refusal = containment_refusal(&lifecycle, "main");

    assert!(refusal.is_some(), "the daemon approved the cleanup");
}
