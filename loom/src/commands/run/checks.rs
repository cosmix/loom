//! Pre-run checks for loom orchestration
//!
//! Contains validation functions that must pass before starting orchestration.

use anyhow::{bail, Result};
use colored::Colorize;
use std::path::Path;

use crate::fs::work_integrity::validate_work_dir_state;
use crate::git::{get_uncommitted_changes_summary, has_uncommitted_changes};

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

/// Check .work directory integrity before starting orchestration
///
/// Validates that .work is in the expected state (directory in main repo,
/// symlink in worktrees). This catches corruption from committed symlinks.
pub fn check_work_dir_integrity(repo_root: &Path) -> Result<()> {
    validate_work_dir_state(repo_root)
}
