//! `loom init --clean` removes stage worktrees through the shared cleanup, never a link.

use super::cleanup::cleanup_worktrees_directory;
use crate::test_git::TestRepo;

#[test]
fn a_symlink_in_worktrees_is_skipped_and_a_real_worktree_is_removed() {
    let repo = TestRepo::new();
    let root = repo.path();
    repo.git(&["worktree", "add", "-q", ".worktrees/s1", "-b", "loom/s1"]);
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("keep.txt"), "keep").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join(".worktrees/link")).unwrap();
    repo.git(&["branch", "loom/link"]);

    cleanup_worktrees_directory(root).unwrap();

    assert!(outside.path().join("keep.txt").is_file());
    repo.git(&["rev-parse", "--verify", "refs/heads/loom/link"]);
    assert!(repo.git(&["branch", "--list", "loom/s1"]).trim().is_empty());
    assert!(!root.join(".worktrees").exists());
}
