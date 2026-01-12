//! Cleanup tests for temporary branches (legacy)
//!
//! Tests for: Cleanup of temp branches after stage completion.
//!
//! Note: With progressive merge, temp branches are no longer created by
//! resolve_base_branch(). These tests verify cleanup of any existing
//! legacy temp branches.

use serial_test::serial;
use std::process::Command;

use loom::git::branch_exists;
use loom::git::cleanup::cleanup_base_branch;
use loom::git::worktree::{resolve_base_branch, ResolvedBase};

use super::helpers::*;

/// Test: Multi-dep stages with all deps merged use main, not temp branch
///
/// With progressive merge, when all dependencies are completed AND merged,
/// the dependent stage uses main as its base (since all work is there).
#[test]
#[serial]
fn test_multi_dep_all_merged_uses_main() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    // Create branches but we'll mark deps as merged (simulating progressive merge)
    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec![]),
        ("stage-c", vec!["stage-a", "stage-b"]),
    ]);

    // Mark both deps as completed AND merged
    complete_stage(&mut graph, "stage-a");
    graph.mark_merged("stage-a").expect("Should mark merged");
    complete_stage(&mut graph, "stage-b");
    graph.mark_merged("stage-b").expect("Should mark merged");

    let result = resolve_base_branch(
        "stage-c",
        &["stage-a".to_string(), "stage-b".to_string()],
        &graph,
        repo_root,
        None,
    )
    .expect("Should succeed");

    // All deps merged - use main as base (no temp branch created)
    assert!(
        matches!(result, ResolvedBase::Main(_)),
        "Expected Main, got {result:?}"
    );

    // No temp branch should exist
    assert!(
        !branch_exists("loom/_base/stage-c", repo_root).expect("Failed to check branch"),
        "No temp branch should be created when deps are merged"
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

    let deleted =
        loom::git::cleanup::cleanup_all_base_branches(repo_root).expect("Failed to cleanup all");

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
