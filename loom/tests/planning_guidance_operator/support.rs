//! Temporary feature repository for the operator guidance proof.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{ensure, Result};
use tempfile::TempDir;

pub fn fixture(target: &str) -> Result<TempDir> {
    let repo = tempfile::tempdir()?;
    let root = repo.path();
    fs::create_dir_all(root.join("doc/product/demo"))?;
    fs::create_dir_all(root.join("doc/plans/demo"))?;
    fs::write(
        root.join("doc/product/demo/index.md"),
        "# Pocket notes\n\nA local plain-text notebook for one person.\n\n\
         ## Specification sections\n\n| # | Section |\n| --- | --- |\n\
         | 01 | Notes |\n| 02 | Search |\n",
    )?;
    fs::write(
        root.join("doc/product/demo/section_01.md"),
        "# Notes\n\nCreate, edit and delete plain-text notes stored locally.\n",
    )?;
    fs::write(
        root.join("doc/plans/demo/index.md"),
        "# Metaplan\n\n## 1. Notes (`PLAN-notes.md`)\n\n\
         Implement local note storage and editing from section 01.\n",
    )?;
    // Starting a missing document must prove a new write, not pass on fixture content.
    if root.join(target).exists() {
        fs::remove_file(root.join(target))?;
    }
    git(root, &["init", "-q", "-b", "main"])?;
    git(root, &["add", "doc"])?;
    git(root, &["commit", "-q", "-m", "seed"])?;
    Ok(repo)
}

fn git(root: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Operator test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()?;
    ensure!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
