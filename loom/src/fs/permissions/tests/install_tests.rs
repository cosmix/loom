//! Tests for hook installation

use crate::fs::permissions::constants::{
    HOOK_COMMIT_GUARD, HOOK_COMMON, HOOK_CONTAINMENT, HOOK_STAGE_LOOKUP,
};
use crate::fs::permissions::hooks::install_loom_hooks_to;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_embedded_commit_guard_hook_is_valid() {
    // The guard uses the installed shared helpers for worktree and stage lookup.
    assert!(HOOK_COMMIT_GUARD.starts_with("#!/usr/bin/env bash"));
    assert!(HOOK_COMMIT_GUARD.contains("/_common.sh"));
    assert!(HOOK_COMMON.contains("/_containment.sh"));
    assert!(HOOK_COMMON.contains("/_stage_lookup.sh"));
    assert!(HOOK_CONTAINMENT.contains("loom_current_worktree()"));
    assert!(HOOK_STAGE_LOOKUP.contains("loom_find_stage_file()"));
    assert!(HOOK_STAGE_LOOKUP.contains("loom_stage_status()"));
    assert!(HOOK_COMMIT_GUARD.contains("loom_current_worktree"));
    assert!(HOOK_COMMIT_GUARD.contains("loom_find_stage_file"));
    assert!(HOOK_COMMIT_GUARD.contains("loom_stage_status"));
    assert!(HOOK_COMMIT_GUARD.contains("check_git_clean"));
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
    assert_eq!(content, HOOK_COMMIT_GUARD);
    assert!(content.contains("loom_current_worktree"));
    assert!(content.contains("LOOM WORKTREE COMPLETION CHECKLIST"));
    for (filename, expected) in [
        ("_common.sh", HOOK_COMMON),
        ("_containment.sh", HOOK_CONTAINMENT),
        ("_stage_lookup.sh", HOOK_STAGE_LOOKUP),
    ] {
        assert_eq!(
            fs::read_to_string(hooks_dir.join(filename)).unwrap(),
            expected
        );
    }

    // Check worktree hooks exist
    assert!(hooks_dir.join("post-tool-use.sh").exists());
    assert!(hooks_dir.join("session-start.sh").exists());
}
