//! Tests for merge execute module

use super::*;

#[test]
fn test_worktree_path() {
    let path = worktree_path("stage-1");
    assert!(path.to_string_lossy().contains(".worktrees"));
    assert!(path.to_string_lossy().contains("stage-1"));
}
