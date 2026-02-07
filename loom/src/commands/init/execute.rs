//! Main execution entry point for loom init command.

use crate::fs::permissions::{ensure_loom_permissions, migrate_legacy_trust};
use crate::fs::work_dir::WorkDir;
use crate::fs::work_integrity::validate_work_dir_state;
use crate::git::install_pre_commit_hook;
use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};

use super::cleanup::{
    cleanup_orphaned_sessions, cleanup_work_directory, cleanup_worktrees_directory,
    prune_stale_worktrees, remove_work_directory_on_failure,
};
use super::plan_setup::initialize_with_plan;

/// RAII guard that cleans up .work directory on drop unless disarmed.
/// This ensures cleanup happens on ANY failure path, not just plan parsing.
struct InitGuard {
    repo_root: PathBuf,
    work_created: bool,
    disarmed: bool,
}

impl InitGuard {
    fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            work_created: false,
            disarmed: false,
        }
    }

    fn mark_work_created(&mut self) {
        self.work_created = true;
    }

    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for InitGuard {
    fn drop(&mut self) {
        if self.work_created && !self.disarmed {
            println!(
                "  {} Cleaning up {} due to initialization failure",
                "→".yellow().bold(),
                ".work/".dimmed()
            );
            remove_work_directory_on_failure(&self.repo_root);
        }
    }
}

/// Initialize the .work/ directory structure
///
/// # Arguments
/// * `plan_path` - Optional path to a plan file to initialize with
/// * `clean` - If true, clean up stale resources before initialization
pub fn execute(plan_path: Option<PathBuf>, clean: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;

    // Validate .work directory state before proceeding
    validate_work_dir_state(&repo_root)?;

    print_header();

    println!("\n{}", "Cleanup".bold());
    println!("{}", "─".repeat(40).dimmed());

    prune_stale_worktrees(&repo_root)?;
    cleanup_orphaned_sessions()?;

    if clean {
        cleanup_work_directory(&repo_root)?;
        cleanup_worktrees_directory(&repo_root)?;
    }

    println!("\n{}", "Initialize".bold());
    println!("{}", "─".repeat(40).dimmed());

    // Create guard to ensure cleanup on any failure after .work is created
    let mut guard = InitGuard::new(repo_root.clone());

    let work_dir = WorkDir::new(".")?;
    work_dir.initialize()?;
    guard.mark_work_created();
    println!(
        "  {} Directory structure created {}",
        "✓".green().bold(),
        ".work/".dimmed()
    );

    // Install git pre-commit hook to prevent .work commits
    match install_pre_commit_hook(&repo_root) {
        Ok(true) => {
            println!("  {} Git pre-commit hook installed", "✓".green().bold());
        }
        Ok(false) => {
            println!(
                "  {} Git pre-commit hook {} up to date",
                "✓".green().bold(),
                "already".dimmed()
            );
        }
        Err(e) => {
            println!(
                "  {} Git pre-commit hook installation failed: {}",
                "!".yellow().bold(),
                e.to_string().dimmed()
            );
            // Non-fatal - continue with init
        }
    }

    ensure_loom_permissions(&repo_root)?;
    println!("  {} Permissions configured", "✓".green().bold());

    // Clean up legacy trustedDirectories entries (no-op if none exist)
    if let Err(e) = migrate_legacy_trust(&repo_root) {
        eprintln!("  {} Legacy trust migration: {}", "!".yellow().bold(), e);
    }

    if let Some(path) = plan_path {
        let stage_count = initialize_with_plan(&work_dir, &path)?;
        print_summary(Some(&path), stage_count);
    } else {
        print_summary(None, 0);
    }

    // Success - disarm the guard to prevent cleanup
    guard.disarm();

    Ok(())
}

/// Print the loom init header
fn print_header() {
    println!();
    println!("{}", "╭──────────────────────────────────────╮".cyan());
    println!(
        "{}",
        "│       Initializing Loom...           │".cyan().bold()
    );
    println!("{}", "╰──────────────────────────────────────╯".cyan());
}

/// Print the final summary
fn print_summary(plan_path: Option<&Path>, stage_count: usize) {
    println!();
    println!("{}", "═".repeat(40).dimmed());

    if let Some(path) = plan_path {
        println!(
            "{} Initialized from {}",
            "✓".green().bold(),
            path.display().to_string().cyan()
        );
        println!(
            "  {} stage{} ready for execution",
            stage_count.to_string().bold(),
            if stage_count == 1 { "" } else { "s" }
        );
    } else {
        println!("{} Empty workspace initialized", "✓".green().bold());
    }

    println!();
    println!("{}", "Next steps:".bold());
    println!("  {}  Start execution", "loom run".cyan());
    println!("  {}  View dashboard", "loom status".cyan());
    println!();
}
