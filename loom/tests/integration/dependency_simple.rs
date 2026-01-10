//! Simple dependency inheritance tests
//!
//! Tests for single dependency chains: A → B

use serial_test::serial;
use std::fs;
use std::process::Command;

use loom::git::create_worktree;
use loom::git::worktree::{resolve_base_branch, ResolvedBase};

use super::helpers::*;

/// Test 1: Simple chain A → B (B inherits from loom/A)
#[test]
#[serial]
fn test_simple_chain_b_inherits_from_a() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);

    let mut graph = build_test_graph(vec![("stage-a", vec![]), ("stage-b", vec!["stage-a"])]);

    complete_stage(&mut graph, "stage-a");

    let result = resolve_base_branch("stage-b", &["stage-a".to_string()], &graph, repo_root, None)
        .expect("Failed to resolve base branch");

    assert_eq!(result, ResolvedBase::Branch("loom/stage-a".to_string()));

    let worktree = create_worktree("stage-b", repo_root, Some(result.branch_name()))
        .expect("Failed to create worktree");

    assert!(
        verify_worktree_has_file(&worktree.path, "a_file.txt"),
        "stage-b worktree should contain a_file.txt from stage-a"
    );
}

/// Test: No dependencies uses main
#[test]
#[serial]
fn test_no_dependencies_uses_main() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    let graph = build_test_graph(vec![("standalone", vec![])]);

    let result =
        resolve_base_branch("standalone", &[], &graph, repo_root, None).expect("Failed to resolve");

    assert_eq!(result, ResolvedBase::Main("main".to_string()));
}

/// Test: Scheduling error when dep not completed
#[test]
#[serial]
fn test_scheduling_error_when_dep_not_completed() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    let graph = build_test_graph(vec![("stage-a", vec![]), ("stage-b", vec!["stage-a"])]);

    let result = resolve_base_branch("stage-b", &["stage-a".to_string()], &graph, repo_root, None);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Scheduling error"),
        "Should be a scheduling error: {err_msg}"
    );
}

/// Test: Worktree inherits full git history
#[test]
#[serial]
fn test_worktree_inherits_full_git_history() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    Command::new("git")
        .args(["checkout", "-b", "loom/stage-a"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout");

    for i in 1..=3 {
        let filename = format!("file{i}.txt");
        fs::write(repo_root.join(&filename), format!("Content {i}")).expect("Failed to write");
        Command::new("git")
            .args(["add", &filename])
            .current_dir(repo_root)
            .output()
            .expect("Failed to add");
        Command::new("git")
            .args(["commit", "-m", &format!("Add {filename}")])
            .current_dir(repo_root)
            .output()
            .expect("Failed to commit");
    }

    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");

    let mut graph = build_test_graph(vec![("stage-a", vec![]), ("stage-b", vec!["stage-a"])]);

    complete_stage(&mut graph, "stage-a");

    let result = resolve_base_branch("stage-b", &["stage-a".to_string()], &graph, repo_root, None)
        .expect("Failed to resolve");

    let worktree = create_worktree("stage-b", repo_root, Some(result.branch_name()))
        .expect("Failed to create worktree");

    assert!(verify_worktree_has_file(&worktree.path, "file1.txt"));
    assert!(verify_worktree_has_file(&worktree.path, "file2.txt"));
    assert!(verify_worktree_has_file(&worktree.path, "file3.txt"));
}

/// Test 4: Dep already merged - fallback to main
#[test]
#[serial]
fn test_dep_already_merged_fallback_to_main() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_file("loom/stage-a", "a_file.txt", "Content from A", repo_root);

    merge_into_main("loom/stage-a", repo_root);
    delete_branch("loom/stage-a", repo_root);

    let mut graph = build_test_graph(vec![("stage-a", vec![]), ("stage-b", vec!["stage-a"])]);

    complete_stage(&mut graph, "stage-a");

    let result = resolve_base_branch("stage-b", &["stage-a".to_string()], &graph, repo_root, None)
        .expect("Failed to resolve base branch");

    assert_eq!(result, ResolvedBase::Main("main".to_string()));

    let worktree = create_worktree("stage-b", repo_root, Some(result.branch_name()))
        .expect("Failed to create worktree");

    assert!(
        verify_worktree_has_file(&worktree.path, "a_file.txt"),
        "stage-b worktree should contain a_file.txt from main (merged from stage-a)"
    );
}
