//! Git-related test helpers

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
#[path = "../../test_support/mod.rs"]
mod test_support;
use tempfile::TempDir;
pub(crate) use test_support::{git_checked, write_main_checkout_identity};

/// Creates a temporary git repository with initial commit
///
/// Returns a TempDir that must be kept in scope for the lifetime of the test
pub fn create_temp_git_repo() -> Result<TempDir> {
    let temp = TempDir::new().context("Failed to create temp directory")?;

    test_support::init_git_repo(temp.path());

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

    let loom_work_dir = work_dir.join(".loom").join("work");
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
