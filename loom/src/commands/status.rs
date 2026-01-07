mod diagnostics;
mod display;
mod validation;

use crate::fs::work_dir::WorkDir;
use anyhow::Result;
use colored::Colorize;

use diagnostics::{
    check_directory_structure, check_orphaned_tracks, check_parsing_errors, check_stuck_runners,
};
use display::{count_files, display_runner_health, display_sessions, display_stages, load_runners};
use validation::{validate_markdown_files, validate_references};

/// Show the status dashboard with context health
pub fn execute() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    println!("{}", "loom Status Dashboard".bold().blue());
    println!("{}", "=".repeat(50));

    let (runners, runner_count) = load_runners(&work_dir)?;
    let track_count = count_files(&work_dir.tracks_dir())?;
    let signal_count = count_files(&work_dir.signals_dir())?;
    let handoff_count = count_files(&work_dir.handoffs_dir())?;
    let stage_count = count_files(&work_dir.stages_dir())?;
    let session_count = count_files(&work_dir.sessions_dir())?;

    println!("\n{}", "Entities".bold());
    println!("  Runners:  {runner_count}");
    println!("  Tracks:   {track_count}");
    println!("  Signals:  {signal_count}");
    println!("  Handoffs: {handoff_count}");

    if stage_count > 0 || session_count > 0 {
        println!("  Stages:   {stage_count}");
        println!("  Sessions: {session_count}");
    }

    if !runners.is_empty() {
        println!("\n{}", "Runner Context Health".bold());
        for runner in runners {
            display_runner_health(&runner);
        }
    }

    if stage_count > 0 {
        display_stages(&work_dir)?;
    }

    if session_count > 0 {
        display_sessions(&work_dir)?;
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

    issues_found += validate_markdown_files(&work_dir.runners_dir(), "runners")?;
    issues_found += validate_markdown_files(&work_dir.tracks_dir(), "tracks")?;
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

    let runners_dir = work_dir.runners_dir();
    let work_root = runners_dir.parent().ok_or_else(|| {
        anyhow::anyhow!("Runners directory has no parent: {}", runners_dir.display())
    })?;

    if !work_root.exists() {
        println!("{} .work directory does not exist", "ERROR:".red().bold());
        println!("  {} Run 'loom init' to create it", "Fix:".yellow());
        return Ok(());
    }

    issues_found += check_directory_structure(&work_dir)?;
    issues_found += check_parsing_errors(&work_dir)?;
    issues_found += check_stuck_runners(&work_dir)?;
    issues_found += check_orphaned_tracks(&work_dir)?;

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
