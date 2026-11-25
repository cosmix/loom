//! Run command - execute plan stages via orchestrator.
//!
//! This module provides commands for running loom plans either in foreground
//! (debugging) or background (daemon) mode.

mod auth_preflight;
pub(crate) mod checks;
mod confinement;
mod daemon_child;
mod foreground;
mod git_preflight;
mod graph_loader;
mod guidance;
mod plan_inputs;
pub(crate) mod sandbox_preflight;
mod signing_preflight;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_checks;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::PathBuf;

use crate::daemon::{DaemonConfig, DaemonServer};
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::{require_tmux, tmux_on_path};

use checks::prepare_repo_for_run;

// Re-export the main entry point for foreground mode
pub use foreground::execute;

// Re-export plan lifecycle functions for daemon use (now from fs module)
pub use crate::fs::plan_lifecycle::mark_plan_done_if_all_merged;

/// Execute orchestrator in background (daemon mode)
/// Usage: `loom run [--manual] [--max-parallel <n>] [--no-merge] [--accept-unix-socket-risk]`
///
/// `daemon_child` is the hidden `--daemon-child <root>`: the daemon process
/// that this command starts runs as `loom run --daemon-child <root>`. It runs
/// no startup preflight, so `accept_unix_socket_risk` never reaches it.
pub fn execute_background(
    manual: bool,
    max_parallel: Option<usize>,
    auto_merge: bool,
    accept_unix_socket_risk: bool,
    daemon_child: Option<PathBuf>,
) -> Result<()> {
    if let Some(root) = daemon_child {
        return daemon_child::execute(&root, daemon_config(manual, max_parallel, auto_merge));
    }

    let work_dir = prepare_background_run(accept_unix_socket_risk)?;

    crate::utils::print_logo_header("Run");

    if DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is already running", "─".dimmed());
        println!();
        guidance::write_follow_guidance(&mut std::io::stdout(), work_dir.root())?;
        print_stop_guidance();
        return Ok(());
    }

    let config = daemon_config(manual, max_parallel, auto_merge);
    let daemon = DaemonServer::with_config(work_dir.root(), config);
    daemon.start()?;

    println!("{} Daemon started", "✓".green().bold());
    if !auto_merge {
        println!("  {} Auto-merge disabled", "→".dimmed());
    }
    println!();
    guidance::write_follow_guidance(&mut std::io::stdout(), work_dir.root())?;
    print_stop_guidance();

    Ok(())
}

/// The daemon's configuration for a background run. `loom run` and the daemon
/// child it starts both build it from the same flags.
fn daemon_config(manual: bool, max_parallel: Option<usize>, auto_merge: bool) -> DaemonConfig {
    DaemonConfig {
        manual_mode: manual,
        max_parallel,
        watch_mode: true, // Background daemon mode continuously watches by design.
        auto_merge,
    }
}

fn prepare_background_run(accept_unix_socket_risk: bool) -> Result<WorkDir> {
    // Ensure git worktree prerequisites are met before starting.
    let repo_root = std::env::current_dir()?;
    crate::git::worktree::require_main_identity(&repo_root)?;
    prepare_repo_for_run(&repo_root)?;

    // Absolute, not ".": a long-lived daemon must get an absolute base — a
    // relative state-directory root silently diverges from every other
    // process's view of the same paths (e.g. `loom attach`, which always
    // resolves absolute).
    let work_dir = WorkDir::new(&repo_root)?;
    work_dir.load()?;
    require_socket_path_fits(&work_dir)?;

    plan_inputs::require_committed_plan(&work_dir)?;

    // Before `DaemonServer::start`: a daemon started without tmux fails every spawn.
    require_tmux(tmux_on_path)?;

    run_startup_preflights(&work_dir, accept_unix_socket_risk)?;
    plan_inputs::mark_plan_in_progress(&work_dir)?;

    // Publish against the committed active filename and the revision stages inherit.
    checks::advisory_source_graph_preflight(&repo_root, &work_dir);

    Ok(work_dir)
}

/// Refuse a run whose daemon could not bind its socket: the resolved socket
/// path must fit `sun_path`. Only the background run checks, before the plan
/// is marked in progress; `loom run --foreground` binds no socket.
fn require_socket_path_fits(work_dir: &WorkDir) -> Result<()> {
    if let Some(problem) = crate::daemon::socket_path_problem(work_dir.root()) {
        bail!("{problem}");
    }
    Ok(())
}

/// The startup preflights both entry points run before the plan is marked in
/// progress, in order: the confinement refusals (`confinement`, plan section
/// 12, and a stage granted every Unix socket unless `accept_unix_socket_risk`);
/// the hard login refusal (`auth_preflight`: claude logged out under the
/// environment stage sessions receive makes every session exit at startup);
/// advisory Remote Control, which never aborts startup; the hard
/// git-version check (merges need `git merge-tree --write-tree`); the hard
/// sandbox-prerequisite check, because like `require_jq` a missing
/// `bwrap`/`socat` or WSL1 makes every session exit at startup, and failing
/// here beats burning the retry budget on a deterministic refusal; the hard
/// signing check (`signing_preflight`: with `commit.gpgsign` true, the daemon
/// must sign unattended in its own environment); then the advisory codex
/// lane. One function, so a refusal added here reaches `loom run` and
/// `loom run --foreground` alike.
fn run_startup_preflights(work_dir: &WorkDir, accept_unix_socket_risk: bool) -> Result<()> {
    confinement::require_confinement(work_dir, accept_unix_socket_risk)?;
    if let Ok(claude_path) = crate::claude::find_claude_path() {
        auth_preflight::require_stage_login(&claude_path)?;
        crate::remote_control::run_startup_preflight(&claude_path, work_dir.root());
    }
    git_preflight::require_min_git_version(work_dir.root())?;
    sandbox_preflight::require_sandbox_prerequisites(work_dir.root())?;
    signing_preflight::require_signing(
        work_dir
            .repo_root()
            .context("cannot resolve the repository root")?,
    )?;
    checks::advisory_codex_lane_preflight(work_dir.root());
    Ok(())
}

fn print_stop_guidance() {
    println!("  {}  Stop daemon", "loom stop".cyan());
}
