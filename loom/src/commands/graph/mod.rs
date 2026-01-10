//! Execution graph display and editing
//!
//! Usage: loom graph [show|edit]
//!
//! ## Module Organization
//!
//! - `display`: Graph building and visual formatting
//! - `indicators`: Status indicators and priority ordering
//! - `levels`: Topological level computation
//! - `colors`: Stage color assignment for visual differentiation

mod colors;
mod display;
mod indicators;
mod levels;
mod tree;

#[cfg(test)]
mod tests;

use anyhow::{bail, Result};
use colored::Colorize;

use crate::verify::transitions::list_all_stages;

// Re-export the public API
pub use colors::stage_color;
pub use display::build_graph_display;
pub use indicators::{status_indicator, status_priority};
pub use levels::compute_stage_levels;
pub use tree::build_tree_display;

/// Show the execution graph
pub fn show() -> Result<()> {
    println!();
    println!("Execution Graph:");
    println!("================");
    println!();

    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    let stages = list_all_stages(&work_dir)?;
    let tree_display = build_tree_display(&stages);
    println!("{tree_display}");

    // Print legend with colored symbols
    println!();
    print!("Legend: ");
    print!("{} ", "✓".green().bold());
    print!("completed  ");
    print!("{} ", "●".blue().bold());
    print!("executing  ");
    print!("{} ", "▶".cyan().bold());
    print!("ready  ");
    print!("{} ", "○".white().dimmed());
    print!("pending  ");
    print!("{} ", "?".magenta().bold());
    print!("waiting  ");
    print!("{} ", "✗".red().bold());
    print!("blocked  ");
    print!("{} ", "⟳".yellow().bold());
    println!("handoff");
    println!();

    Ok(())
}

/// Edit the execution graph by opening the stages directory
///
/// The execution graph is dynamically built from stage files in `.work/stages/`.
/// This command opens the stages directory in the configured editor, allowing
/// direct modification of stage files.
pub fn edit() -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    let stages_dir = work_dir.join("stages");
    if !stages_dir.exists() {
        bail!("No stages directory found. Run 'loom init <plan>' first.");
    }

    // Check if there are any stage files
    let stage_files: Vec<_> = std::fs::read_dir(&stages_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();

    if stage_files.is_empty() {
        bail!("No stage files found. Run 'loom init <plan>' first.");
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    println!(
        "The execution graph is built from stage files in: {}",
        stages_dir.display()
    );
    println!();
    println!("Stage files:");
    for entry in &stage_files {
        println!("  - {}", entry.path().display());
    }
    println!();
    println!("To edit a stage, run:");
    println!("  {editor} {}/[stage-id].md", stages_dir.display());
    println!();
    println!("Each stage file contains YAML frontmatter with:");
    println!("  - id, name, status, dependencies, parallel_group, acceptance, files");

    Ok(())
}
