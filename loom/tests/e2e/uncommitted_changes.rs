//! E2E tests for loom run uncommitted changes check
//!
//! These tests verify that `loom run` properly fails when there are
//! uncommitted changes in the repository (issue #10).

use anyhow::{Context, Result};
use loom::commands::run;
use serial_test::serial;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use super::fixtures::plans::minimal_plan;

/// Create a temporary git repository for testing
fn create_test_git_repo() -> Result<TempDir> {
    let temp = TempDir::new().context("Failed to create temp directory")?;

    Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .output()
        .context("Failed to run git init")?;

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp.path())
        .output()
        .context("Failed to set git user.email")?;

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp.path())
        .output()
        .context("Failed to set git user.name")?;

    fs::write(temp.path().join("README.md"), "# Test Repository\n")
        .context("Failed to write README.md")?;

    Command::new("git")
        .args(["add", "."])
        .current_dir(temp.path())
        .output()
        .context("Failed to git add")?;

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp.path())
        .output()
        .context("Failed to git commit")?;

    Ok(temp)
}

/// Initialize loom with a plan and create necessary config
fn setup_loom(repo_root: &Path, plan_content: &str) -> Result<()> {
    // Create directory structure
    let doc_plans_dir = repo_root.join("doc").join("plans");
    fs::create_dir_all(&doc_plans_dir).context("Failed to create doc/plans directory")?;

    let plan_path = doc_plans_dir.join("PLAN-0001-test.md");
    fs::write(&plan_path, plan_content).context("Failed to write plan file")?;

    // Create .work directory structure
    let loom_work_dir = repo_root.join(".work");
    let subdirs = [
        "runners",
        "tracks",
        "signals",
        "handoffs",
        "archive",
        "stages",
        "sessions",
        "logs",
        "crashes",
        "checkpoints",
        "task-state",
    ];

    for subdir in &subdirs {
        let path = loom_work_dir.join(subdir);
        fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create {subdir} directory"))?;
    }

    // Create config.toml that references the plan
    let config_content = format!(
        r#"# loom Configuration
# Generated from plan: {}

[plan]
source_path = "{}"
plan_id = "0001"
plan_name = "Test Plan"
base_branch = "main"
"#,
        plan_path.display(),
        plan_path.display()
    );

    let config_path = loom_work_dir.join("config.toml");
    fs::write(config_path, config_content).context("Failed to write config.toml")?;

    Ok(())
}

/// Test that loom run allows untracked files (new files not in git)
/// but fails when tracked files are modified
#[test]
#[serial]
fn test_loom_run_allows_untracked_files() {
    use loom::git::has_uncommitted_changes;

    let temp_repo = create_test_git_repo().expect("Should create test repo");
    let repo_root = temp_repo.path();

    // Create an untracked file
    fs::write(repo_root.join("untracked.txt"), "Untracked content\n")
        .expect("Should write untracked file");

    // Verify git doesn't consider untracked files as "uncommitted changes"
    let has_changes =
        has_uncommitted_changes(repo_root).expect("Should check for uncommitted changes");
    assert!(
        !has_changes,
        "Untracked files should not be considered uncommitted changes"
    );
}

/// Test that loom run fails when there are staged but uncommitted changes
#[test]
#[serial]
fn test_loom_run_fails_with_staged_changes() {
    let temp_repo = create_test_git_repo().expect("Should create test repo");
    let repo_root = temp_repo.path();

    // Initialize loom with a minimal plan
    setup_loom(repo_root, &minimal_plan()).expect("Should initialize loom");

    // Create and stage changes
    fs::write(repo_root.join("staged.txt"), "Staged content\n").expect("Should write test file");

    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(repo_root)
        .output()
        .expect("Should git add");

    // Change to the test repo directory for the run command
    let original_dir = std::env::current_dir().expect("Should get current dir");
    std::env::set_current_dir(repo_root).expect("Should change directory");

    // Try to run loom - should fail due to staged changes
    let result = run::execute(
        false, // manual
        None,  // max_parallel
        false, // watch
        true,  // auto_merge
    );

    // Restore original directory
    std::env::set_current_dir(original_dir).expect("Should restore directory");

    // Verify it failed with the correct error
    assert!(result.is_err(), "loom run should fail with staged changes");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Uncommitted changes"),
        "Error should mention uncommitted changes, got: {err_msg}"
    );
}

/// Test that loom run fails when there are both staged and unstaged changes
#[test]
#[serial]
fn test_loom_run_fails_with_mixed_changes() {
    let temp_repo = create_test_git_repo().expect("Should create test repo");
    let repo_root = temp_repo.path();

    // Initialize loom with a minimal plan
    setup_loom(repo_root, &minimal_plan()).expect("Should initialize loom");

    // Create staged changes
    fs::write(repo_root.join("staged.txt"), "Staged content\n").expect("Should write staged file");

    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(repo_root)
        .output()
        .expect("Should git add");

    // Create unstaged changes
    fs::write(repo_root.join("unstaged.txt"), "Unstaged content\n")
        .expect("Should write unstaged file");

    // Change to the test repo directory for the run command
    let original_dir = std::env::current_dir().expect("Should get current dir");
    std::env::set_current_dir(repo_root).expect("Should change directory");

    // Try to run loom - should fail due to mixed changes
    let result = run::execute(
        false, // manual
        None,  // max_parallel
        false, // watch
        true,  // auto_merge
    );

    // Restore original directory
    std::env::set_current_dir(original_dir).expect("Should restore directory");

    // Verify it failed with the correct error
    assert!(result.is_err(), "loom run should fail with mixed changes");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Uncommitted changes"),
        "Error should mention uncommitted changes, got: {err_msg}"
    );
}

/// Test that loom run passes uncommitted changes check when repo is clean
#[test]
#[serial]
fn test_loom_run_passes_check_with_clean_repo() {
    use loom::git::has_uncommitted_changes;

    let temp_repo = create_test_git_repo().expect("Should create test repo");
    let repo_root = temp_repo.path();

    // Create changes but commit them
    fs::write(repo_root.join("committed.txt"), "Committed content\n")
        .expect("Should write test file");

    Command::new("git")
        .args(["add", "committed.txt"])
        .current_dir(repo_root)
        .output()
        .expect("Should git add");

    Command::new("git")
        .args(["commit", "-m", "Add committed file"])
        .current_dir(repo_root)
        .output()
        .expect("Should git commit");

    // Verify the git check passes (no uncommitted changes)
    let has_changes =
        has_uncommitted_changes(repo_root).expect("Should check for uncommitted changes");

    assert!(
        !has_changes,
        "Clean repo should not have uncommitted changes"
    );
}

/// Test that loom run fails when there are modified tracked files
#[test]
#[serial]
fn test_loom_run_fails_with_modified_tracked_files() {
    let temp_repo = create_test_git_repo().expect("Should create test repo");
    let repo_root = temp_repo.path();

    // Initialize loom with a minimal plan
    setup_loom(repo_root, &minimal_plan()).expect("Should initialize loom");

    // Modify an existing tracked file (README.md)
    fs::write(repo_root.join("README.md"), "# Modified Test Repository\n")
        .expect("Should modify README.md");

    // Change to the test repo directory for the run command
    let original_dir = std::env::current_dir().expect("Should get current dir");
    std::env::set_current_dir(repo_root).expect("Should change directory");

    // Try to run loom - should fail due to modified file
    let result = run::execute(
        false, // manual
        None,  // max_parallel
        false, // watch
        true,  // auto_merge
    );

    // Restore original directory
    std::env::set_current_dir(original_dir).expect("Should restore directory");

    // Verify it failed with the correct error
    assert!(
        result.is_err(),
        "loom run should fail with modified tracked files"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Uncommitted changes"),
        "Error should mention uncommitted changes, got: {err_msg}"
    );
}
