mod helpers;
pub mod serialization;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;

use crate::fs::work_dir::WorkDir;
use crate::models::runner::Runner;
use crate::models::runner::RunnerStatus;
use crate::validation::{validate_id, validate_name};

use helpers::{find_runner_file, generate_runner_id, load_runner, truncate};
use serialization::runner_to_markdown;

/// Create a new runner
pub fn create(name: String, runner_type: String) -> Result<()> {
    // Validate inputs before any file operations
    validate_name(&name).context("Invalid runner name")?;
    validate_name(&runner_type).context("Invalid runner type")?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let id = generate_runner_id(&work_dir, &runner_type)?;

    let mut runner = Runner::new(name.clone(), runner_type.clone());
    runner.id = id.clone();

    let markdown = runner_to_markdown(&runner)?;
    let file_path = work_dir.runners_dir().join(format!("{id}.md"));

    fs::write(&file_path, markdown)
        .with_context(|| format!("Failed to write runner file: {id}.md"))?;

    println!("{}", "Runner created successfully!".green().bold());
    println!("  {} {}", "ID:".bold(), id.cyan());
    println!("  {} {}", "Name:".bold(), name);
    println!("  {} {}", "Type:".bold(), runner_type);
    println!(
        "  {} {}",
        "File:".bold(),
        format!(".work/runners/{id}.md").dimmed()
    );

    Ok(())
}

/// List all runners
pub fn list(show_inactive: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let runners_dir = work_dir.runners_dir();
    let entries = fs::read_dir(&runners_dir).context("Failed to read runners directory")?;

    let mut runners = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_runner(&path) {
                Ok(runner) => {
                    if show_inactive || runner.status != RunnerStatus::Archived {
                        runners.push(runner);
                    }
                }
                Err(e) => {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    eprintln!(
                        "{} {}: {}",
                        "Warning: Failed to parse".yellow(),
                        filename,
                        e
                    );
                }
            }
        }
    }

    if runners.is_empty() {
        println!("{}", "No runners found.".yellow());
        return Ok(());
    }

    runners.sort_by(|a, b| a.id.cmp(&b.id));

    println!("\n{}", "Runners".bold().underline());
    println!();
    println!(
        "{:<12} {:<25} {:<25} {:<10} {:<20} {:<10}",
        "ID".bold(),
        "Name".bold(),
        "Type".bold(),
        "Status".bold(),
        "Track".bold(),
        "Context".bold()
    );
    println!("{}", "-".repeat(105).dimmed());

    for runner in runners {
        let status_str = match runner.status {
            RunnerStatus::Idle => "Idle".dimmed().to_string(),
            RunnerStatus::Active => "Active".green().to_string(),
            RunnerStatus::Blocked => "Blocked".yellow().to_string(),
            RunnerStatus::Archived => "Archived".red().to_string(),
        };

        let track_str = runner
            .assigned_track
            .as_ref()
            .map(|t| t.cyan().to_string())
            .unwrap_or_else(|| "-".dimmed().to_string());

        let context_pct = runner.context_health();
        let context_str = format!("{context_pct:.1}%");
        let context_colored = if context_pct > 85.0 {
            context_str.red()
        } else if context_pct > 70.0 {
            context_str.yellow()
        } else {
            context_str.green()
        };

        println!(
            "{:<12} {:<25} {:<25} {:<10} {:<20} {:<10}",
            runner.id.cyan(),
            truncate(&runner.name, 25),
            truncate(&runner.runner_type, 25),
            status_str,
            track_str,
            context_colored
        );
    }

    println!();
    Ok(())
}

/// Assign a runner to a track
pub fn assign(runner_id: String, track_id: String) -> Result<()> {
    // Validate IDs before any file operations
    validate_id(&runner_id).context("Invalid runner ID")?;
    validate_id(&track_id).context("Invalid track ID")?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let runner_path = find_runner_file(&work_dir, &runner_id)?;
    let mut runner = load_runner(&runner_path)?;

    let track_path = work_dir.tracks_dir().join(format!("{track_id}.md"));
    if !track_path.exists() {
        bail!("Track '{track_id}' does not exist");
    }

    runner.assign_to_track(track_id.clone());

    let markdown = runner_to_markdown(&runner)?;
    fs::write(&runner_path, markdown)
        .with_context(|| format!("Failed to update runner file: {runner_id}.md"))?;

    println!("{}", "Runner assigned successfully!".green().bold());
    println!("  {} {}", "Runner:".bold(), runner_id.cyan());
    println!("  {} {}", "Track:".bold(), track_id.cyan());

    Ok(())
}

/// Release a runner from its track
pub fn release(runner_id: String) -> Result<()> {
    // Validate ID before any file operations
    validate_id(&runner_id).context("Invalid runner ID")?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let runner_path = find_runner_file(&work_dir, &runner_id)?;
    let mut runner = load_runner(&runner_path)?;

    let previous_track = runner.assigned_track.clone();
    runner.release_from_track();

    let markdown = runner_to_markdown(&runner)?;
    fs::write(&runner_path, markdown)
        .with_context(|| format!("Failed to update runner file: {runner_id}.md"))?;

    println!("{}", "Runner released successfully!".green().bold());
    println!("  {} {}", "Runner:".bold(), runner_id.cyan());
    if let Some(track) = previous_track {
        println!("  {} {}", "From track:".bold(), track.cyan());
    }

    Ok(())
}

/// Archive a runner
pub fn archive(runner_id: String) -> Result<()> {
    // Validate ID before any file operations
    validate_id(&runner_id).context("Invalid runner ID")?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let runner_path = find_runner_file(&work_dir, &runner_id)?;
    let mut runner = load_runner(&runner_path)?;

    runner.archive();

    let archive_dir = work_dir.archive_dir().join("runners");
    fs::create_dir_all(&archive_dir).context("Failed to create archive/runners directory")?;

    let archive_path = archive_dir.join(format!("{runner_id}.md"));
    let markdown = runner_to_markdown(&runner)?;

    fs::write(&archive_path, markdown)
        .with_context(|| format!("Failed to write archived runner: {runner_id}.md"))?;

    fs::remove_file(&runner_path)
        .with_context(|| format!("Failed to remove original runner file: {runner_id}.md"))?;

    println!("{}", "Runner archived successfully!".green().bold());
    println!("  {} {}", "Runner:".bold(), runner_id.cyan());
    println!(
        "  {} {}",
        "Archived to:".bold(),
        format!(".work/archive/runners/{runner_id}.md").dimmed()
    );

    Ok(())
}
