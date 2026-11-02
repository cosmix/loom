//! Conflict detection and resolution tests
//!
//! Tests for unmerged dependency refusal and manually prepared worktree bases.

use serial_test::serial;
use std::fs;

use loom::git::worktree::resolve_base_branch;

use super::helpers::*;

/// Test 5: Multi-dep not merged returns scheduling error
///
/// With progressive merge, stages can only be scheduled after ALL their dependencies
/// are completed AND merged. If deps are completed but not merged, this is a
/// scheduling error (not a merge conflict - that would be detected during merge).
#[test]
#[serial]
fn test_multi_dep_not_merged_returns_scheduling_error() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "shared.txt", "Content from A", repo_root);
    create_branch_with_file("loom/stage-b", "shared.txt", "Content from B", repo_root);

    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec![]),
        ("stage-c", vec!["stage-a", "stage-b"]),
    ]);

    complete_stage(&mut graph, "stage-a");
    complete_stage(&mut graph, "stage-b");
    // Note: deps are completed but NOT merged - this is a scheduling error

    let result = resolve_base_branch(
        "stage-c",
        &["stage-a".to_string(), "stage-b".to_string()],
        &graph,
        repo_root,
        None,
    );

    assert!(result.is_err(), "Should fail due to unmerged dependencies");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Scheduling error") && err_msg.contains("merged=false"),
        "Error should mention scheduling error with unmerged deps: {err_msg}"
    );
}

/// The scheduling error identifies the affected stage or dependency.
#[test]
#[serial]
fn test_unmerged_dependency_error_contains_helpful_info() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/dep-x", "conflict.rs", "fn x() {}", repo_root);
    create_branch_with_file("loom/dep-y", "conflict.rs", "fn y() {}", repo_root);

    let mut graph = build_test_graph(vec![
        ("dep-x", vec![]),
        ("dep-y", vec![]),
        ("blocked-stage", vec!["dep-x", "dep-y"]),
    ]);

    complete_stage(&mut graph, "dep-x");
    complete_stage(&mut graph, "dep-y");

    let result = resolve_base_branch(
        "blocked-stage",
        &["dep-x".to_string(), "dep-y".to_string()],
        &graph,
        repo_root,
        None,
    );

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();

    assert!(
        err_msg.contains("blocked-stage") || err_msg.contains("dep-"),
        "Error should reference stage or deps: {err_msg}"
    );
}

/// Create `branch` from main, merge both conflicting stage branches into it,
/// and commit the manual resolution of `shared.txt`.
fn resolve_conflict_on_base_branch(branch: &str, repo_root: &std::path::Path) {
    git_checked(repo_root, &["checkout", "main"]);
    git_checked(repo_root, &["checkout", "-b", branch]);
    git_checked(
        repo_root,
        &["merge", "--no-ff", "-m", "Merge stage-a", "loom/stage-a"],
    );
    let merge_output = git_unchecked(
        repo_root,
        &["merge", "--no-ff", "-m", "Merge stage-b", "loom/stage-b"],
    );
    assert!(
        !merge_output.status.success(),
        "fixture must produce a merge conflict"
    );
    fs::write(repo_root.join("shared.txt"), "Line from A\nLine from B\n")
        .expect("Failed to write resolved file");
    git_checked(repo_root, &["add", "shared.txt"]);
    git_checked(repo_root, &["commit", "-m", "Resolve merge conflict"]);
    git_checked(repo_root, &["checkout", "main"]);
}

/// A manually resolved base branch can seed a worktree.
#[test]
#[serial]
fn test_worktree_uses_manually_resolved_base_branch() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "shared.txt", "Line from A", repo_root);
    create_branch_with_file("loom/stage-b", "shared.txt", "Line from B", repo_root);

    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec![]),
        ("stage-c", vec!["stage-a", "stage-b"]),
    ]);

    complete_stage(&mut graph, "stage-a");
    complete_stage(&mut graph, "stage-b");

    let first_attempt = resolve_base_branch(
        "stage-c",
        &["stage-a".to_string(), "stage-b".to_string()],
        &graph,
        repo_root,
        None,
    );
    assert!(
        first_attempt.is_err(),
        "Unmerged dependencies must refuse scheduling"
    );
    let error = first_attempt.unwrap_err().to_string();
    assert!(
        error.contains("Scheduling error") && error.contains("merged=false"),
        "{error}"
    );

    resolve_conflict_on_base_branch("loom/_base/stage-c", repo_root);

    let worktree = create_worktree_isolated("stage-c", repo_root, Some("loom/_base/stage-c"))
        .expect("Failed to create worktree after conflict resolution");

    let content =
        fs::read_to_string(worktree.path.join("shared.txt")).expect("Failed to read shared.txt");
    assert!(
        content.contains("Line from A"),
        "Should have content from A"
    );
    assert!(
        content.contains("Line from B"),
        "Should have content from B"
    );
}
