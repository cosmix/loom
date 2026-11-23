//! Contract: verified cleanup checks the worktree HEAD even while the stage
//! branch exists and is merged, so a detached HEAD holding commits the target
//! lacks keeps the worktree.

use super::*;
use crate::test_git::{git, TestRepo};
use std::fs;

#[test]
fn a_detached_head_with_unique_commits_blocks_verified_cleanup() {
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
    fs::write(worktree.join("feature.txt"), "feature\n").unwrap();
    git(&worktree, &["add", "feature.txt"]);
    git(&worktree, &["commit", "-m", "feature"]);
    let commit = git(&worktree, &["rev-parse", "HEAD"]);
    git(root, &["merge", "--no-ff", "-m", "merge stage", "loom/s"]);
    git(&worktree, &["checkout", "--detach"]);
    fs::write(worktree.join("later.txt"), "later\n").unwrap();
    git(&worktree, &["add", "later.txt"]);
    git(&worktree, &["commit", "-m", "later"]);

    let result = cleanup_verified_stage("s", &commit, "main", root);

    assert!(result.is_err(), "cleanup proceeded: {result:?}");
    assert!(worktree.is_dir());
    assert!(worktree.join("later.txt").exists());
    git(root, &["rev-parse", "--verify", "refs/heads/loom/s"]);
}
