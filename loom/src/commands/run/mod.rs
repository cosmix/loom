//! Run command - execute plan stages via orchestrator.
//!
//! This module provides commands for running loom plans either in foreground
//! (debugging) or background (daemon) mode.

mod config_ops;
mod filename_ops;
mod foreground;
mod frontmatter;
mod graph_loader;
mod merge_status;
mod plan_lifecycle;

#[cfg(test)]
mod tests;

use anyhow::{bail, Result};
use colored::Colorize;

use crate::daemon::{DaemonConfig, DaemonServer};
use crate::fs::work_dir::WorkDir;
use crate::git::{get_uncommitted_changes_summary, has_uncommitted_changes};

// Re-export the main entry point for foreground mode
pub use foreground::execute;

/// Execute orchestrator in background (daemon mode)
/// Usage: loom run [--manual] [--max-parallel <n>] [--watch] [--no-merge]
pub fn execute_background(
    manual: bool,
    max_parallel: Option<usize>,
    _watch: bool, // Daemon always runs in watch mode; CLI flag is accepted but ignored
    auto_merge: bool,
) -> Result<()> {
    // Check for uncommitted changes before starting
    let repo_root = std::env::current_dir()?;
    check_for_uncommitted_changes(&repo_root)?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // Mark plan as in-progress when starting execution
    plan_lifecycle::mark_plan_in_progress(&work_dir)?;

    if DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is already running", "─".dimmed());
        println!();
        println!("  {}  Check status", "loom status".cyan());
        println!("  {}  Stop daemon", "loom stop".cyan());
        return Ok(());
    }

    let daemon_config = DaemonConfig {
        manual_mode: manual,
        max_parallel,
        watch_mode: true, // Daemon always runs in watch mode (ignores CLI flag)
        auto_merge,
    };

    let daemon = DaemonServer::with_config(work_dir.root(), daemon_config);
    daemon.start()?;

    println!("{} Daemon started", "✓".green().bold());
    if !auto_merge {
        println!("  {} Auto-merge disabled", "→".dimmed());
    }
    println!();
    println!("  {}  Monitor progress", "loom status".cyan());
    println!("  {}  Stop daemon", "loom stop".cyan());

    Ok(())
}

/// Check for uncommitted changes and bail if found
fn check_for_uncommitted_changes(repo_root: &std::path::Path) -> Result<()> {
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
