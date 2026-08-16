//! `loom knowledge sync` — rebuild derived context artifacts.
//!
//! Writes only through the context store, never into the knowledge tree
//! itself: `refresh` rebuilds the catalog when stale and persists it to the
//! cache, and is a no-op when the catalog is already current.

use super::context::resolve;
use crate::context::refresh::{refresh, RefreshOutcome};
use anyhow::Result;
use colored::Colorize;

/// Rebuild derived context artifacts when the knowledge tree has changed.
pub fn sync(structural_only: bool, json: bool) -> Result<()> {
    let (knowledge_root, store) = resolve()?;
    let outcome = refresh(&store, &knowledge_root, structural_only)?;

    if json {
        print_json(&outcome)
    } else {
        print_human(&outcome);
        Ok(())
    }
}

fn print_json(outcome: &RefreshOutcome) -> Result<()> {
    let payload = serde_json::json!({
        "rebuilt": outcome.rebuilt,
        "revision": outcome.structural.revision,
        "files": outcome.report.as_ref().map(|report| report.files),
        "chunks": outcome.report.as_ref().map(|report| report.chunks),
        "issues": outcome.report.as_ref().map(|report| report.issues.len()),
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn print_human(outcome: &RefreshOutcome) {
    if outcome.rebuilt {
        println!("{} Rebuilt the context catalog", "✓".green().bold());
    } else {
        println!("{} Catalog already current", "─".dimmed());
    }
    println!("  {} {}", "Revision:".cyan(), outcome.structural.revision);

    if let Some(report) = &outcome.report {
        println!("  {} {}", "Files:".cyan(), report.files);
        println!("  {} {}", "Chunks:".cyan(), report.chunks);
        println!("  {} {}", "Issues:".cyan(), report.issues.len());
    }
}
