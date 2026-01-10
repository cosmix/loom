//! Conflict detection and resolution tests
//!
//! Tests for: Multi-dep conflict blocking and retry after resolution

use serial_test::serial;
use std::fs;
use std::process::Command;

use loom::git::create_worktree;
use loom::git::worktree::resolve_base_branch;

use super::helpers::*;

/// Test 5: Multi-dep conflict returns error
#[test]
#[serial]
fn test_multi_dep_conflict_returns_error() {
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

    let result = resolve_base_branch(
        "stage-c",
        &["stage-a".to_string(), "stage-b".to_string()],
        &graph,
        repo_root,
        None,
    );

    assert!(result.is_err(), "Should fail due to merge conflict");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Merge conflict") || err_msg.contains("merge failed"),
        "Error should mention merge conflict: {err_msg}"
    );
}

/// Test: Conflict error contains helpful info
#[test]
#[serial]
fn test_conflict_error_contains_helpful_info() {
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

/// Test 6: Retry after manual conflict resolution
#[test]
#[serial]
fn test_retry_after_manual_conflict_resolution() {
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
        "First attempt should fail due to conflict"
    );

    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");

    Command::new("git")
        .args(["checkout", "-b", "loom/_base/stage-c"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to create base branch");

    Command::new("git")
        .args(["merge", "--no-ff", "-m", "Merge stage-a", "loom/stage-a"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to merge stage-a");

    let merge_output = Command::new("git")
        .args(["merge", "--no-ff", "-m", "Merge stage-b", "loom/stage-b"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to start merge");

    if !merge_output.status.success() {
        fs::write(repo_root.join("shared.txt"), "Line from A\nLine from B\n")
            .expect("Failed to write resolved file");

        Command::new("git")
            .args(["add", "shared.txt"])
            .current_dir(repo_root)
            .output()
            .expect("Failed to add");

        Command::new("git")
            .args(["commit", "-m", "Resolve merge conflict"])
            .current_dir(repo_root)
            .output()
            .expect("Failed to commit resolution");
    }

    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");

    let worktree = create_worktree("stage-c", repo_root, Some("loom/_base/stage-c"))
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
