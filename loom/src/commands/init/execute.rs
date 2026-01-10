//! Main execution entry point for loom init command.

use crate::fs::permissions::{add_worktrees_to_global_trust, ensure_loom_permissions};
use crate::fs::work_dir::WorkDir;
use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};

use super::cleanup::{
    cleanup_orphaned_tmux_sessions, cleanup_work_directory, cleanup_worktrees_directory,
    prune_stale_worktrees,
};
use super::plan_setup::initialize_with_plan;

/// Initialize the .work/ directory structure
///
/// # Arguments
/// * `plan_path` - Optional path to a plan file to initialize with
/// * `clean` - If true, clean up stale resources before initialization
pub fn execute(plan_path: Option<PathBuf>, clean: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;

    print_header();

    println!("\n{}", "Cleanup".bold());
    println!("{}", "─".repeat(40).dimmed());

    prune_stale_worktrees(&repo_root)?;
    cleanup_orphaned_tmux_sessions()?;

    if clean {
        cleanup_work_directory(&repo_root)?;
        cleanup_worktrees_directory(&repo_root)?;
    }

    println!("\n{}", "Initialize".bold());
    println!("{}", "─".repeat(40).dimmed());

    let work_dir = WorkDir::new(".")?;
    work_dir.initialize()?;
    println!(
        "  {} Directory structure created {}",
        "✓".green().bold(),
        ".work/".dimmed()
    );

    ensure_loom_permissions(&repo_root)?;
    println!("  {} Permissions configured", "✓".green().bold());

    add_worktrees_to_global_trust(&repo_root)?;
    println!("  {} Worktrees directory trusted", "✓".green().bold());

    if let Some(path) = plan_path {
        let stage_count = initialize_with_plan(&work_dir, &path)?;
        print_summary(Some(&path), stage_count);
    } else {
        print_summary(None, 0);
    }

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
