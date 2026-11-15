//! Temporary feature repository for the operator guidance proof.

use std::fs;

use anyhow::Result;
use tempfile::TempDir;

#[path = "../test_support/mod.rs"]
mod test_support;

use test_support::git_repo::{git, TestRepo};

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
    TestRepo::builder()
        .identity("Operator test", "test@example.com")
        .no_seed()
        .init_in(root);
    git(root, &["add", "doc"]);
    git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "-m",
            "seed",
        ],
    );
    Ok(repo)
}
