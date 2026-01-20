//! Tests for permission constants

use crate::fs::permissions::constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};

#[test]
fn test_loom_permissions_constant() {
    // Main repo includes all permissions (shared with worktrees via symlink)
    assert!(LOOM_PERMISSIONS.contains(&"Bash(loom:*)"));
    // Now includes worktree permissions so settings can be symlinked
    assert!(LOOM_PERMISSIONS.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Write(.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Read(../../.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Write(../../.work/**)"));
}

#[test]
fn test_worktree_permissions_constant() {
    // Worktree permissions should match main repo permissions
    // (settings.json includes permissions for worktree access patterns)
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(../../.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(../../.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.claude/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom:*)"));
}
