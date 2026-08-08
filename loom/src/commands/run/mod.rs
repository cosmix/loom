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

use anyhow::{bail, Result};
use colored::Colorize;

use crate::daemon::{DaemonConfig, DaemonServer};
use crate::fs::plan_lifecycle;
use crate::fs::work_dir::{read_terminal_config, write_terminal_config, WorkDir};
use crate::models::session::{SessionBackendKind, TerminalConfig};

use checks::prepare_repo_for_run;

// Re-export the main entry point for foreground mode
pub use foreground::execute;

// Re-export plan lifecycle functions for daemon use (now from fs module)
pub use crate::fs::plan_lifecycle::mark_plan_done_if_all_merged;

/// Execute orchestrator in background (daemon mode)
/// Usage: loom run [--manual] [--max-parallel <n>] [--watch] [--no-merge] [--backend <native|tmux>]
pub fn execute_background(
    manual: bool,
    max_parallel: Option<usize>,
    _watch: bool, // Daemon always runs in watch mode; CLI flag is accepted but ignored
    auto_merge: bool,
    backend: Option<String>,
) -> Result<()> {
    // Ensure git worktree prerequisites are met before starting.
    let repo_root = std::env::current_dir()?;
    prepare_repo_for_run(&repo_root)?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // Resolve --backend: persist an explicit selection, guarding against
    // desync with an already-running daemon (its backend is fixed at
    // construction, so a config flip alone cannot reach it). `loom run`
    // never prompts — only `loom init` does.
    if let Some(value) = backend {
        let requested = match value.as_str() {
            "native" => SessionBackendKind::Native,
            "tmux" => SessionBackendKind::Tmux,
            other => bail!("Invalid terminal backend: {other}"),
        };

        let persisted = read_terminal_config(work_dir.root())?.backend;

        if DaemonServer::is_running(work_dir.root()) && requested != persisted {
            println!(
                "{} backend change requires a restart: run `loom stop`, then `loom run --backend {}`",
                "─".dimmed(),
                value
            );
        } else {
            if requested == SessionBackendKind::Tmux {
                // An explicit re-selection is a request to retry tmux.
                crate::orchestrator::terminal::backend::clear_fallback_marker(work_dir.root());
            }
            write_terminal_config(work_dir.root(), &TerminalConfig { backend: requested })?;
        }
    }

    // Advisory tmux preflight — never aborts startup.
    if read_terminal_config(work_dir.root())?.backend == SessionBackendKind::Tmux
        && which::which("tmux").is_err()
    {
        eprintln!(
            "tmux backend selected but tmux not found - sessions will fail to spawn until tmux is \
             installed or the backend is set back to native"
        );
    }

    // Advisory Remote Control preflight — never aborts startup.
    if let Ok(claude_path) = crate::claude::find_claude_path() {
        crate::remote_control::run_startup_preflight(&claude_path, work_dir.root());
    }

    // Advisory Codex lane preflight — never aborts startup.
    checks::advisory_codex_lane_preflight(work_dir.root());

    // Mark plan as in-progress when starting execution
    plan_lifecycle::mark_plan_in_progress(&work_dir)?;

    crate::utils::print_logo_header("Run");

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
