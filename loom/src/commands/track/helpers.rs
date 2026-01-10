use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use crate::fs::work_dir::WorkDir;
use crate::models::track::TrackStatus;
use crate::parser::markdown::MarkdownDocument;

/// Generate a slug from a track name
pub fn slug_from_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Find a track file by ID or name slug
pub fn find_track_file(work_dir: &WorkDir, id_or_name: &str) -> Result<PathBuf> {
    let tracks_dir = work_dir.tracks_dir();
    let entries = fs::read_dir(&tracks_dir).context("Failed to read tracks directory")?;

    let slug = slug_from_name(id_or_name);
    let slug_path = tracks_dir.join(format!("{slug}.md"));

    if slug_path.exists() {
        return Ok(slug_path);
    }

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        if let Ok(doc) = MarkdownDocument::parse(&content) {
            if let Some(track_id) = doc.get_frontmatter("id") {
                if track_id == id_or_name {
                    return Ok(path);
                }
            }
        }
    }

    bail!("Track not found: {id_or_name}")
}

/// Format a TrackStatus for display with colors
pub fn format_status(status: &TrackStatus) -> String {
    match status {
        TrackStatus::Active => "Active".green().to_string(),
        TrackStatus::Blocked => "Blocked".yellow().to_string(),
        TrackStatus::Completed => "Completed".blue().to_string(),
        TrackStatus::Archived => "Archived".dimmed().to_string(),
    }
}

/// Convert TrackStatus to string representation
pub fn status_to_string(status: &TrackStatus) -> &'static str {
    match status {
        TrackStatus::Active => "Active",
        TrackStatus::Blocked => "Blocked",
        TrackStatus::Completed => "Completed",
        TrackStatus::Archived => "Archived",
    }
}

/// Parse TrackStatus from string
pub fn status_from_string(s: &str) -> Result<TrackStatus> {
    match s {
        "Active" => Ok(TrackStatus::Active),
        "Blocked" => Ok(TrackStatus::Blocked),
        "Completed" => Ok(TrackStatus::Completed),
        "Archived" => Ok(TrackStatus::Archived),
        _ => bail!("Invalid status: {s}"),
    }
}

/// Parse a datetime string in RFC3339 format
pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .context("Failed to parse datetime")
}

/// Format a datetime for display
pub fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M UTC").to_string()
}
