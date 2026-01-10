//! Cleanup tests for temporary branches
//!
//! Tests for: Cleanup of temp branches after stage completion

use serial_test::serial;
use std::process::Command;

use loom::git::worktree::{resolve_base_branch, ResolvedBase};
use loom::git::branch_exists;
use loom::git::cleanup::cleanup_base_branch;

use super::helpers::*;

/// Test 7: Cleanup temp branch after completion
#[test]
#[serial]
fn test_cleanup_temp_branch_after_completion() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "a.txt", "A", repo_root);
    create_branch_with_file("loom/stage-b", "b.txt", "B", repo_root);

    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec![]),
        ("stage-c", vec!["stage-a", "stage-b"]),
    ]);

    complete_stage(&mut graph, "stage-a");
    complete_stage(&mut graph, "stage-b");

    let result = resolve_base_branch(
        "stage-c",
        &["stage-a".to_string(), "stage-b".to_string()],
        &graph,
        repo_root,
        None,
    )
    .expect("Should succeed");

    assert!(matches!(result, ResolvedBase::TempMerge(_)));

    assert!(
        branch_exists("loom/_base/stage-c", repo_root).expect("Failed to check branch"),
        "Temp branch should exist before cleanup"
    );

    let cleaned = cleanup_base_branch("stage-c", repo_root)
        .expect("Failed to cleanup");

    assert!(cleaned, "Should have deleted the branch");

    assert!(
        !branch_exists("loom/_base/stage-c", repo_root).expect("Failed to check branch"),
        "Temp branch should not exist after cleanup"
    );
}

/// Test: Cleanup all temp branches
#[test]
#[serial]
fn test_cleanup_all_temp_branches() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    Command::new("git")
        .args(["branch", "loom/_base/stage-1"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to create branch");

    Command::new("git")
        .args(["branch", "loom/_base/stage-2"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to create branch");

    Command::new("git")
        .args(["branch", "loom/_base/stage-3"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to create branch");

    assert!(branch_exists("loom/_base/stage-1", repo_root).unwrap());
    assert!(branch_exists("loom/_base/stage-2", repo_root).unwrap());
    assert!(branch_exists("loom/_base/stage-3", repo_root).unwrap());

    let deleted = loom::git::cleanup::cleanup_all_base_branches(repo_root)
        .expect("Failed to cleanup all");

    assert_eq!(deleted.len(), 3, "Should delete all 3 temp branches");

    assert!(!branch_exists("loom/_base/stage-1", repo_root).unwrap());
    assert!(!branch_exists("loom/_base/stage-2", repo_root).unwrap());
    assert!(!branch_exists("loom/_base/stage-3", repo_root).unwrap());
}

/// Test: Cleanup nonexistent branch returns false
#[test]
#[serial]
fn test_cleanup_nonexistent_branch() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    let cleaned = cleanup_base_branch("nonexistent", repo_root)
        .expect("Cleanup should not error on nonexistent branch");

    assert!(!cleaned, "Should return false for nonexistent branch");
}
