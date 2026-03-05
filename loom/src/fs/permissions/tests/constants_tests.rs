//! Tests for permission constants

use crate::fs::permissions::constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};

#[test]
fn test_loom_permissions_constant() {
    // Main repo permissions - tightened to minimum necessary
    assert!(LOOM_PERMISSIONS.contains(&"Bash(loom *)"));
    assert!(LOOM_PERMISSIONS.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Write(.work/**)"));
    // Only CLAUDE.md files, not all of .claude/
    assert!(LOOM_PERMISSIONS.contains(&"Read(.claude/CLAUDE.md)"));
    assert!(LOOM_PERMISSIONS.contains(&"Read(~/.claude/CLAUDE.md)"));
    // Loom hooks only, not all hooks
    assert!(LOOM_PERMISSIONS.contains(&"Read(~/.claude/hooks/loom/**)"));
}

#[test]
fn test_worktree_permissions_constant() {
    // Worktree permissions - same tightened set
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(.work/**)"));
    // Only CLAUDE.md files, not all of .claude/
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.claude/CLAUDE.md)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/CLAUDE.md)"));
    // Loom hooks only
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/hooks/loom/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom *)"));
}
