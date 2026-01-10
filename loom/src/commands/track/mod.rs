mod helpers;
pub mod serialization;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;

use crate::fs::work_dir::WorkDir;
use crate::models::track::{Track, TrackStatus};
use crate::validation::{validate_description, validate_name};

use helpers::{find_track_file, format_datetime, format_status, slug_from_name};
use serialization::{track_from_markdown, track_to_markdown};

/// Create a new track
pub fn create(name: String, description: Option<String>) -> Result<()> {
    // Validate inputs before any file operations
    validate_name(&name).context("Invalid track name")?;
    if let Some(ref desc) = description {
        validate_description(desc).context("Invalid track description")?;
    }

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let track = Track::new(name, description);
    let slug = slug_from_name(&track.name);
    let track_path = work_dir.tracks_dir().join(format!("{slug}.md"));

    if track_path.exists() {
        bail!("Track with slug '{slug}' already exists");
    }

    let markdown = track_to_markdown(&track)?;
    fs::write(&track_path, markdown).context("Failed to write track file")?;

    println!(
        "{} Created track: {} ({})",
        "ok".green().bold(),
        track.name.bold(),
        track.id.dimmed()
    );
    println!("  File: {}", format!(".work/tracks/{slug}.md").dimmed());

    Ok(())
}

/// List all tracks
pub fn list(show_archived: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let tracks_dir = work_dir.tracks_dir();
    let entries = fs::read_dir(&tracks_dir).context("Failed to read tracks directory")?;

    let mut tracks = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read track file: {filename}"))?;

        match track_from_markdown(&content) {
            Ok(track) => {
                if show_archived || track.status != TrackStatus::Archived {
                    tracks.push(track);
                }
            }
            Err(e) => {
                eprintln!("{} Failed to parse {}: {}", "!".yellow(), filename, e);
            }
        }
    }

    if tracks.is_empty() {
        println!("No tracks found.");
        return Ok(());
    }

    tracks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    println!("{}", "Tracks".bold().blue());
    println!("{}", "=".repeat(80));
    println!(
        "{:<35} {:<12} {:<15} {}",
        "ID".bold(),
        "Status".bold(),
        "Runner".bold(),
        "Updated".bold()
    );
    println!("{}", "-".repeat(80));

    for track in tracks {
        let status_str = format_status(&track.status);
        let runner_str = track.assigned_runner.as_deref().unwrap_or("-");
        let updated_str = format_datetime(&track.updated_at);

        println!(
            "{:<35} {:<20} {:<15} {}",
            track.id, status_str, runner_str, updated_str
        );
    }

    Ok(())
}

/// Show details of a specific track
pub fn show(id: String) -> Result<()> {
    // Validate name (track IDs are derived from names via slug)
    validate_name(&id).context("Invalid track ID or name")?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let track_path = find_track_file(&work_dir, &id)?;
    let content = fs::read_to_string(&track_path).context("Failed to read track file")?;

    let track = track_from_markdown(&content)?;

    println!("{}", track.name.bold().blue());
    println!("{}", "=".repeat(60));
    println!("{:<15} {}", "ID:".bold(), track.id);
    println!("{:<15} {}", "Status:".bold(), format_status(&track.status));

    if let Some(ref runner) = track.assigned_runner {
        println!("{:<15} {}", "Runner:".bold(), runner);
    }

    if let Some(ref parent) = track.parent_track {
        println!("{:<15} {}", "Parent:".bold(), parent);
    }

    if !track.child_tracks.is_empty() {
        println!(
            "{:<15} {}",
            "Children:".bold(),
            track.child_tracks.join(", ")
        );
    }

    println!(
        "{:<15} {}",
        "Created:".bold(),
        format_datetime(&track.created_at)
    );
    println!(
        "{:<15} {}",
        "Updated:".bold(),
        format_datetime(&track.updated_at)
    );

    if let Some(closed_at) = track.closed_at {
        println!("{:<15} {}", "Closed:".bold(), format_datetime(&closed_at));
    }

    if let Some(ref reason) = track.close_reason {
        println!("{:<15} {}", "Close Reason:".bold(), reason);
    }

    if let Some(ref desc) = track.description {
        println!("\n{}", "Description".bold());
        println!("{}", "-".repeat(60));
        println!("{desc}");
    }

    Ok(())
}

/// Close a track
pub fn close(id: String, reason: Option<String>) -> Result<()> {
    // Validate inputs before any file operations
    validate_name(&id).context("Invalid track ID or name")?;
    if let Some(ref r) = reason {
        validate_description(r).context("Invalid close reason")?;
    }

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let track_path = find_track_file(&work_dir, &id)?;
    let content = fs::read_to_string(&track_path).context("Failed to read track file")?;

    let mut track = track_from_markdown(&content)?;

    if track.status == TrackStatus::Completed {
        bail!("Track '{}' is already closed", track.name);
    }

    track.close(reason);

    let markdown = track_to_markdown(&track)?;
    fs::write(&track_path, markdown).context("Failed to write track file")?;

    println!(
        "{} Closed track: {} ({})",
        "ok".green().bold(),
        track.name.bold(),
        track.id.dimmed()
    );

    if let Some(ref close_reason) = track.close_reason {
        println!("  Reason: {close_reason}");
    }

    Ok(())
}
