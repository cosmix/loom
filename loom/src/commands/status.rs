pub mod common;
pub mod data;
mod diagnostics;
mod display;
pub mod merge_status;
pub mod render;
pub mod ui;
mod validation;

use crate::daemon::DaemonServer;
use crate::fs::work_dir::WorkDir;
use anyhow::Result;
use colored::Colorize;

use diagnostics::{check_directory_structure, check_parsing_errors};
use display::count_files;
use validation::{validate_markdown_files, validate_references};

/// Show the status dashboard with context health
pub fn execute(live: bool, compact: bool, verbose: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let work_path = work_dir.root();

    // Compact mode: single-line output for scripting
    if compact {
        return execute_compact(&work_dir);
    }

    // Live mode: subscribe to daemon for real-time updates
    if live {
        if DaemonServer::is_running(work_path) {
            return ui::run_tui(work_path);
        } else {
            eprintln!("{}", "Daemon not running. Cannot use --live mode.".yellow());
            println!(
                "{}",
                "Start the daemon with 'loom run' or use static mode.".dimmed()
            );
            return Ok(());
        }
    }

    // Static mode (default): show snapshot of current state
    execute_static(&work_dir, verbose)
}

/// Execute compact mode - single line output for scripting
fn execute_compact(work_dir: &WorkDir) -> Result<()> {
    use data::collect_status_data;
    use std::io::stdout;

    let status_data = collect_status_data(work_dir)?;
    render::render_compact(&mut stdout(), &status_data)?;
    Ok(())
}

/// Show static status dashboard
fn execute_static(work_dir: &WorkDir, verbose: bool) -> Result<()> {
    use data::collect_status_data;
    use std::io::stdout;

    let status_data = collect_status_data(work_dir)?;
    let mut out = stdout();
    let stage_count = count_files(&work_dir.stages_dir())?;

    println!();
    println!("{}", "Loom Status".bold().blue());
    println!("{}", "─".repeat(40));

    // Show daemon status hint
    if DaemonServer::is_running(work_dir.root()) {
        println!(
            "{}",
            "Use 'loom status --live' for real-time updates".dimmed()
        );
    } else {
        println!(
            "{}",
            "Daemon not running (use 'loom run' to start)".dimmed()
        );
    }

    // Progress bar with stage counts
    render::render_progress(&mut out, &status_data.progress)?;

    // Unified stage graph (replaces separate Active Stages, Worktrees, Merge sections)
    if stage_count > 0 {
        println!();
        render::render_graph(&mut out, &status_data)?;
    }

    // Merge status: only show if there are pending merges or conflicts
    if !status_data.merge.pending.is_empty() || !status_data.merge.conflicts.is_empty() {
        render::render_merge_status(&mut out, &status_data.merge)?;
    }

    // Verbose mode: show detailed failure information
    if verbose {
        render::render_attention(&mut out, &status_data.stages)?;
    }

    println!();
    Ok(())
}

/// Validate the integrity of the work directory
pub fn validate() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    println!("{}", "Validating work directory...".bold());

    let mut issues_found = 0;

    issues_found += validate_markdown_files(&work_dir.signals_dir(), "signals")?;
    issues_found += validate_markdown_files(&work_dir.handoffs_dir(), "handoffs")?;

    issues_found += validate_references(&work_dir)?;

    if issues_found == 0 {
        println!("\n{}", "All validations passed!".green().bold());
    } else {
        println!(
            "\n{} {}",
            "Found".red().bold(),
            format!("{issues_found} issue(s)").red().bold()
        );
    }

    Ok(())
}

/// Diagnose issues with the work directory
pub fn doctor() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    println!("{}", "Running diagnostics...".bold());

    let mut issues_found = 0;

    let work_root = work_dir.root();

    if !work_root.exists() {
        println!("{} .work directory does not exist", "ERROR:".red().bold());
        println!("  {} Run 'loom init' to create it", "Fix:".yellow());
        return Ok(());
    }

    issues_found += check_directory_structure(&work_dir)?;
    issues_found += check_parsing_errors(&work_dir)?;

    if issues_found == 0 {
        println!("\n{}", "No issues found!".green().bold());
    } else {
        println!(
            "\n{} {}",
            "Found".yellow().bold(),
            format!("{issues_found} potential issue(s)").yellow().bold()
        );
    }

    Ok(())
}
