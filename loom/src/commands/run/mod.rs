//! Run command - execute plan stages via orchestrator.
//!
//! This module provides commands for running loom plans either in foreground
//! (debugging) or background (daemon) mode.

mod foreground;
mod frontmatter;
mod graph_loader;

#[cfg(test)]
mod tests;

use anyhow::Result;
use colored::Colorize;

use crate::daemon::{DaemonConfig, DaemonServer};
use crate::fs::work_dir::WorkDir;

// Re-export the main entry point for foreground mode
pub use foreground::execute;

/// Execute orchestrator in background (daemon mode)
/// Usage: loom run [--stage <id>] [--manual] [--max-parallel <n>] [--watch] [--auto-merge]
pub fn execute_background(
    stage_id: Option<String>,
    manual: bool,
    max_parallel: Option<usize>,
    _watch: bool, // Daemon always runs in watch mode; CLI flag is accepted but ignored
    auto_merge: bool,
) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    if let Some(ref id) = stage_id {
        println!("{} Running single stage: {}", "→".cyan().bold(), id.bold());
    }

    if DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is already running", "─".dimmed());
        println!();
        println!("  {}  Check status", "loom status".cyan());
        println!("  {}  Stop daemon", "loom stop".cyan());
        return Ok(());
    }

    let daemon_config = DaemonConfig {
        stage_id: stage_id.clone(),
        manual_mode: manual,
        max_parallel,
        watch_mode: true, // Daemon always runs in watch mode (ignores CLI flag)
        auto_merge,
    };

    let daemon = DaemonServer::with_config(work_dir.root(), daemon_config);
    daemon.start()?;

    println!("{} Daemon started", "✓".green().bold());
    if auto_merge {
        println!("  {} Auto-merge enabled", "→".dimmed());
    }
    println!();
    println!("  {}  Monitor progress", "loom status".cyan());
    println!("  {}  Stop daemon", "loom stop".cyan());

    Ok(())
}
