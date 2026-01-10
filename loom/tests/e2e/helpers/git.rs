//! Git-related test helpers

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Creates a temporary git repository with initial commit
///
/// Returns a TempDir that must be kept in scope for the lifetime of the test
pub fn create_temp_git_repo() -> Result<TempDir> {
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

    std::fs::write(temp.path().join("README.md"), "# Test Repository\n")
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

/// Initializes loom with a plan document
///
/// Creates the necessary directory structure and writes the plan file
pub fn init_loom_with_plan(work_dir: &Path, plan_content: &str) -> Result<PathBuf> {
    let doc_plans_dir = work_dir.join("doc").join("plans");
    std::fs::create_dir_all(&doc_plans_dir).context("Failed to create doc/plans directory")?;

    let plan_path = doc_plans_dir.join("PLAN-0001-test.md");
    std::fs::write(&plan_path, plan_content).context("Failed to write plan file")?;

    let loom_work_dir = work_dir.join(".work");
    let subdirs = [
        "runners", "tracks", "signals", "handoffs", "archive", "stages", "sessions",
    ];

    for subdir in &subdirs {
        let path = loom_work_dir.join(subdir);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create {subdir} directory"))?;
    }

    Ok(plan_path)
}
