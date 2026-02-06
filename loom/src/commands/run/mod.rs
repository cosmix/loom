//! Run command - execute plan stages via orchestrator.
//!
//! This module provides commands for running loom plans either in foreground
//! (debugging) or background (daemon) mode.

mod checks;
mod foreground;
mod frontmatter;
mod graph_loader;

#[cfg(test)]
mod tests;

use anyhow::Result;
use colored::Colorize;

use crate::daemon::{DaemonConfig, DaemonServer};
use crate::fs::plan_lifecycle;
use crate::fs::work_dir::WorkDir;

use checks::check_for_uncommitted_changes;

// Re-export the main entry point for foreground mode
pub use foreground::execute;

// Re-export plan lifecycle functions for daemon use (now from fs module)
pub use crate::fs::plan_lifecycle::mark_plan_done_if_all_merged;

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

    // Detect terminal BEFORE daemonizing (daemon loses terminal context after fork)
    // Store in environment variable so it can be read back after the fork
    if let Ok(terminal) = crate::orchestrator::terminal::native::detect_terminal() {
        // SAFETY: This runs in main() before the tokio runtime spawns any threads,
        // so there are no concurrent readers of the environment.
        unsafe { std::env::set_var("LOOM_TERMINAL", terminal.display_name()) };
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
