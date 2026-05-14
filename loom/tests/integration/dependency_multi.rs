//! Multiple dependency and diamond pattern tests
//!
//! Tests for: [A, B] → C and A → B, A → C, [B,C] → D patterns
//!
//! With progressive merge, multi-dep stages use main as base when all deps
//! are completed AND merged. Temp merge branches are no longer created.

use serial_test::serial;
use std::fs;
use std::process::Command;

use loom::git::worktree::{resolve_base_branch, ResolvedBase};
use loom::git::{branch_exists, create_worktree};

use super::helpers::*;

/// Test 2: Multiple deps [A, B] → C with all deps merged uses main
///
/// With progressive merge, when all deps are merged, C uses main as base.
#[test]
#[serial]
fn test_multiple_deps_all_merged_uses_main() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    // Create and merge both dep branches into main
    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);
    merge_into_main("loom/stage-a", repo_root);

    create_branch_with_file("loom/stage-b", "b_file.txt", "Content from B", repo_root);
    merge_into_main("loom/stage-b", repo_root);

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
    .expect("Failed to resolve base branch");

    // All deps merged - use main as base
    assert!(
        matches!(result, ResolvedBase::Main(_)),
        "Expected Main, got {result:?}"
    );
    assert_eq!(result.branch_name(), "main");

    // No temp branch should be created
    assert!(
        !branch_exists("loom/_base/stage-c", repo_root).expect("Failed to check branch"),
        "No temp merge branch should be created"
    );

    // Create worktree from main and verify it has both files
    let worktree = create_worktree("stage-c", repo_root, Some(result.branch_name()))
        .expect("Failed to create worktree");

    assert!(
        verify_worktree_has_file(&worktree.path, "a_file.txt"),
        "stage-c worktree should contain a_file.txt from stage-a (via main)"
    );
    assert!(
        verify_worktree_has_file(&worktree.path, "b_file.txt"),
        "stage-c worktree should contain b_file.txt from stage-b (via main)"
    );
}

/// Test: Multi-dep with deps not merged returns scheduling error
#[test]
#[serial]
fn test_multiple_deps_not_merged_returns_error() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec![]),
        ("stage-c", vec!["stage-a", "stage-b"]),
    ]);

    // Complete deps but don't mark as merged
    complete_stage(&mut graph, "stage-a");
    complete_stage(&mut graph, "stage-b");
    // Note: NOT calling mark_merged()

    let result = resolve_base_branch(
        "stage-c",
        &["stage-a".to_string(), "stage-b".to_string()],
        &graph,
        repo_root,
        None,
    );

    assert!(result.is_err(), "Should return error when deps not merged");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Scheduling error"),
        "Error should mention scheduling"
    );
}

/// Test 3: Diamond pattern A → B, A → C, [B,C] → D with all merged
#[test]
#[serial]
fn test_diamond_pattern_all_merged() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    // Create A and merge
    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);
    merge_into_main("loom/stage-a", repo_root);

    // Create B from main (which now has A's work) and merge
    Command::new("git")
        .args(["checkout", "-b", "loom/stage-b"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to create branch");
    fs::write(repo_root.join("b_file.txt"), "Content from B").expect("Failed to write");
    Command::new("git")
        .args(["add", "b_file.txt"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to add");
    Command::new("git")
        .args(["commit", "-m", "Add b_file.txt"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to commit");
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");
    merge_into_main("loom/stage-b", repo_root);

    // Create C from main and merge
    Command::new("git")
        .args(["checkout", "-b", "loom/stage-c"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to create branch");
    fs::write(repo_root.join("c_file.txt"), "Content from C").expect("Failed to write");
    Command::new("git")
        .args(["add", "c_file.txt"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to add");
    Command::new("git")
        .args(["commit", "-m", "Add c_file.txt"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to commit");
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");
    merge_into_main("loom/stage-c", repo_root);

    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec!["stage-a"]),
        ("stage-c", vec!["stage-a"]),
        ("stage-d", vec!["stage-b", "stage-c"]),
    ]);

    // Mark all as completed and merged
    complete_stage(&mut graph, "stage-a");
    graph.mark_merged("stage-a").expect("Should mark merged");
    complete_stage(&mut graph, "stage-b");
    graph.mark_merged("stage-b").expect("Should mark merged");
    complete_stage(&mut graph, "stage-c");
    graph.mark_merged("stage-c").expect("Should mark merged");

    let result = resolve_base_branch(
        "stage-d",
        &["stage-b".to_string(), "stage-c".to_string()],
        &graph,
        repo_root,
        None,
    )
    .expect("Failed to resolve base branch");

    // All deps merged - use main
    assert!(
        matches!(result, ResolvedBase::Main(_)),
        "Expected Main for diamond, got {result:?}"
    );

    let worktree = create_worktree("stage-d", repo_root, Some(result.branch_name()))
        .expect("Failed to create worktree");

    assert!(
        verify_worktree_has_file(&worktree.path, "a_file.txt"),
        "stage-d should have a_file.txt from stage-a (via main)"
    );
    assert!(
        verify_worktree_has_file(&worktree.path, "b_file.txt"),
        "stage-d should have b_file.txt from stage-b (via main)"
    );
    assert!(
        verify_worktree_has_file(&worktree.path, "c_file.txt"),
        "stage-d should have c_file.txt from stage-c (via main)"
    );
}

/// Test: All deps completed and merged - uses main
#[test]
#[serial]
fn test_all_deps_merged_uses_main() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);
    merge_into_main("loom/stage-a", repo_root);
    delete_branch("loom/stage-a", repo_root);

    create_branch_with_file("loom/stage-b", "b_file.txt", "Content from B", repo_root);
    merge_into_main("loom/stage-b", repo_root);
    delete_branch("loom/stage-b", repo_root);

    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec![]),
        ("stage-c", vec!["stage-a", "stage-b"]),
    ]);

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
    .expect("Failed to resolve base branch");

    assert_eq!(result, ResolvedBase::Main("main".to_string()));
}
