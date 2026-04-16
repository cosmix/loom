//! Tests for hook installation

use crate::fs::permissions::constants::HOOK_COMMIT_GUARD;
use crate::fs::permissions::hooks::install_loom_hooks_to;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_embedded_commit_guard_hook_is_valid() {
    // Verify the embedded hook script has correct shebang and key functions
    assert!(HOOK_COMMIT_GUARD.starts_with("#!/usr/bin/env bash"));
    // Check for key functions that should exist
    assert!(HOOK_COMMIT_GUARD.contains("detect_loom_worktree"));
    assert!(HOOK_COMMIT_GUARD.contains("check_git_clean"));
    assert!(HOOK_COMMIT_GUARD.contains("get_stage_status"));
    assert!(HOOK_COMMIT_GUARD.contains("warn_with_reason"));
}

#[test]
fn test_install_loom_hooks_creates_hook_files() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks/loom");

    let result = install_loom_hooks_to(&hooks_dir);
    assert!(result.is_ok());

    // Check hooks are installed to the temp directory
    let commit_guard_path = hooks_dir.join("commit-guard.sh");
    assert!(commit_guard_path.exists());
    let content = fs::read_to_string(&commit_guard_path).unwrap();
    assert!(content.contains("detect_loom_worktree"));
    assert!(content.contains("LOOM WORKTREE EXIT BLOCKED"));

    // Check worktree hooks exist
    assert!(hooks_dir.join("post-tool-use.sh").exists());
    assert!(hooks_dir.join("session-start.sh").exists());
}
