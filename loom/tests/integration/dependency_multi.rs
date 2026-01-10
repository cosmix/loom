//! Multiple dependency and diamond pattern tests
//!
//! Tests for: [A, B] → C and A → B, A → C, [B,C] → D patterns

use serial_test::serial;
use std::fs;
use std::process::Command;

use loom::git::worktree::{resolve_base_branch, ResolvedBase};
use loom::git::{branch_exists, create_worktree};

use super::helpers::*;

/// Test 2: Multiple deps [A, B] → C (C gets merged base)
#[test]
#[serial]
fn test_multiple_deps_c_gets_merged_base() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);
    create_branch_with_file("loom/stage-b", "b_file.txt", "Content from B", repo_root);

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
    .expect("Failed to resolve base branch");

    assert!(
        matches!(result, ResolvedBase::TempMerge(_)),
        "Expected TempMerge, got {:?}",
        result
    );
    assert_eq!(result.branch_name(), "loom/_base/stage-c");

    assert!(
        branch_exists("loom/_base/stage-c", repo_root).expect("Failed to check branch"),
        "Temp merge branch should exist"
    );

    let worktree = create_worktree("stage-c", repo_root, Some(result.branch_name()))
        .expect("Failed to create worktree");

    assert!(
        verify_worktree_has_file(&worktree.path, "a_file.txt"),
        "stage-c worktree should contain a_file.txt from stage-a"
    );
    assert!(
        verify_worktree_has_file(&worktree.path, "b_file.txt"),
        "stage-c worktree should contain b_file.txt from stage-b"
    );
}

/// Test 3: Diamond pattern A → B, A → C, [B,C] → D
#[test]
#[serial]
fn test_diamond_pattern() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);

    Command::new("git")
        .args(["checkout", "loom/stage-a"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout");
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

    Command::new("git")
        .args(["checkout", "loom/stage-a"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout");
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

    let mut graph = build_test_graph(vec![
        ("stage-a", vec![]),
        ("stage-b", vec!["stage-a"]),
        ("stage-c", vec!["stage-a"]),
        ("stage-d", vec!["stage-b", "stage-c"]),
    ]);

    complete_stage(&mut graph, "stage-a");
    complete_stage(&mut graph, "stage-b");
    complete_stage(&mut graph, "stage-c");

    let result = resolve_base_branch(
        "stage-d",
        &["stage-b".to_string(), "stage-c".to_string()],
        &graph,
        repo_root,
        None,
    )
    .expect("Failed to resolve base branch");

    assert!(
        matches!(result, ResolvedBase::TempMerge(_)),
        "Expected TempMerge for diamond, got {:?}",
        result
    );

    let worktree = create_worktree("stage-d", repo_root, Some(result.branch_name()))
        .expect("Failed to create worktree");

    assert!(
        verify_worktree_has_file(&worktree.path, "a_file.txt"),
        "stage-d should have a_file.txt from stage-a (via stage-b and stage-c)"
    );
    assert!(
        verify_worktree_has_file(&worktree.path, "b_file.txt"),
        "stage-d should have b_file.txt from stage-b"
    );
    assert!(
        verify_worktree_has_file(&worktree.path, "c_file.txt"),
        "stage-d should have c_file.txt from stage-c"
    );
}

/// Test: Multiple deps some already merged
#[test]
#[serial]
fn test_multiple_deps_some_already_merged() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);
    create_branch_with_file("loom/stage-b", "b_file.txt", "Content from B", repo_root);

    merge_into_main("loom/stage-a", repo_root);
    delete_branch("loom/stage-a", repo_root);

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
    .expect("Failed to resolve base branch");

    assert_eq!(result, ResolvedBase::Branch("loom/stage-b".to_string()));
}

/// Test: All deps already merged - fallback to main
#[test]
#[serial]
fn test_all_deps_already_merged_fallback_to_main() {
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
    complete_stage(&mut graph, "stage-b");

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
