//! Pre-run checks for loom orchestration
//!
//! Contains validation functions that must pass before starting orchestration.

use anyhow::{bail, Result};
use colored::Colorize;
use std::path::Path;

use crate::git::{get_uncommitted_changes_summary, has_uncommitted_changes};

/// Ensure the repository is ready for Loom's git worktree operations.
pub fn prepare_repo_for_run(repo_root: &Path) -> Result<()> {
    let repo_bootstrap = crate::git::ensure_repo_ready_for_worktrees(repo_root)?;
    print_repo_bootstrap(repo_bootstrap);
    check_for_uncommitted_changes(repo_root)
}

fn print_repo_bootstrap(result: crate::git::RepoBootstrapResult) {
    if !result.changed() {
        return;
    }

    if result.initialized_repo {
        println!("{} Initialized git repository", "✓".green().bold());
    }

    if result.created_initial_commit {
        println!(
            "{} Created bootstrap commit for worktree support",
            "✓".green().bold()
        );
    }
}

/// Check for uncommitted changes and bail if found
///
/// This prevents starting orchestration with a dirty repository, which could
/// cause issues with worktree creation and branch management.
pub fn check_for_uncommitted_changes(repo_root: &Path) -> Result<()> {
    if has_uncommitted_changes(repo_root)? {
        let summary = get_uncommitted_changes_summary(repo_root)?;
        eprintln!(
            "{} Cannot start loom run with uncommitted changes",
            "✗".red().bold()
        );
        eprintln!();
        if !summary.is_empty() {
            for line in summary.lines() {
                eprintln!("  {}", line.dimmed());
            }
            eprintln!();
        }
        eprintln!("  {} Commit or stash your changes first:", "→".dimmed());
        eprintln!(
            "    {}  Commit changes",
            "git commit -am \"message\"".cyan()
        );
        eprintln!("    {}  Or stash them", "git stash".cyan());
        bail!("Uncommitted changes in repository - commit or stash before running loom");
    }
    Ok(())
}
