//! Shared test helpers for dependency inheritance integration tests

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use loom::plan::graph::ExecutionGraph;
use loom::plan::schema::StageDefinition;

/// Test helper: Create a temporary git repository with initial commit
pub fn init_test_repo() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let repo_root = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to init git repo");

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to set git user.email");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to set git user.name");

    fs::write(repo_root.join("README.md"), "# Test Repository\n")
        .expect("Failed to write README.md");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git add");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git commit");

    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to rename branch to main");

    temp_dir
}

/// Test helper: Create a branch with a commit adding a file
pub fn create_branch_with_file(name: &str, filename: &str, content: &str, repo_root: &Path) {
    Command::new("git")
        .args(["checkout", "-b", name])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout new branch");

    fs::write(repo_root.join(filename), content).expect("Failed to write file");

    Command::new("git")
        .args(["add", filename])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git add");

    Command::new("git")
        .args(["commit", "-m", &format!("Add {filename}")])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git commit");

    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");
}

/// Test helper: Build execution graph from stage definitions
pub fn build_test_graph(stages: Vec<(&str, Vec<&str>)>) -> ExecutionGraph {
    let stage_defs: Vec<StageDefinition> = stages
        .into_iter()
        .map(|(id, deps)| StageDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: Some(format!("Test stage {id}")),
            dependencies: deps.into_iter().map(String::from).collect(),
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            parallel_group: None,
            auto_merge: None,
        })
        .collect();

    ExecutionGraph::build(stage_defs).expect("Failed to build graph")
}

/// Test helper: Mark stage as completed in graph
pub fn complete_stage(graph: &mut ExecutionGraph, stage_id: &str) {
    graph
        .mark_executing(stage_id)
        .expect("Failed to mark executing");
    graph
        .mark_completed(stage_id)
        .expect("Failed to mark completed");
}

/// Test helper: Verify worktree contains file from dependency branch
pub fn verify_worktree_has_file(worktree_path: &Path, filename: &str) -> bool {
    worktree_path.join(filename).exists()
}

/// Test helper: Merge a branch into main
pub fn merge_into_main(branch: &str, repo_root: &Path) {
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");

    Command::new("git")
        .args(["merge", "--no-ff", "-m", &format!("Merge {branch}"), branch])
        .current_dir(repo_root)
        .output()
        .expect("Failed to merge");
}

/// Test helper: Delete a branch
pub fn delete_branch(branch: &str, repo_root: &Path) {
    Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_root)
        .output()
        .expect("Failed to delete branch");
}
