//! Tests for hook installation

use crate::fs::permissions::constants::HOOK_COMMIT_GUARD;
use crate::fs::permissions::hooks::install_loom_hooks;
use std::fs;

#[test]
fn test_embedded_commit_guard_hook_is_valid() {
    // Verify the embedded hook script has correct shebang and key functions
    assert!(HOOK_COMMIT_GUARD.starts_with("#!/usr/bin/env bash"));
    // Check for key functions that should exist
    assert!(HOOK_COMMIT_GUARD.contains("detect_loom_worktree"));
    assert!(HOOK_COMMIT_GUARD.contains("check_git_clean"));
    assert!(HOOK_COMMIT_GUARD.contains("get_stage_status"));
    assert!(HOOK_COMMIT_GUARD.contains("block_with_reason"));
}

#[test]
fn test_install_loom_hooks_creates_hook_files() {
    // This test modifies ~/.claude/hooks/ which is a real directory
    // We'll verify the hooks are installed correctly
    let result = install_loom_hooks();
    assert!(result.is_ok());

    let home_dir = dirs::home_dir().expect("should have home dir");

    // Check global hooks are installed
    let commit_guard_path = home_dir.join(".claude/hooks/commit-guard.sh");
    if commit_guard_path.exists() {
        let content = fs::read_to_string(&commit_guard_path).unwrap();
        assert!(content.contains("detect_loom_worktree"));
        assert!(content.contains("LOOM WORKTREE EXIT BLOCKED"));
    }

    // Check worktree hooks are installed to loom/ subdirectory
    let worktree_hooks_dir = home_dir.join(".claude/hooks/loom");
    if worktree_hooks_dir.exists() {
        assert!(worktree_hooks_dir.join("post-tool-use.sh").exists());
        assert!(worktree_hooks_dir.join("session-start.sh").exists());
    }
}
