//! Execution graph display
//!
//! Usage: loom graph
//!
//! ## Module Organization
//!
//! - `display`: Graph building and visual formatting
//! - `indicators`: Status indicators and priority ordering
//! - `levels`: Topological level computation
//! - `colors`: Stage color assignment for visual differentiation

pub mod colors;
mod display;
pub mod indicators;
mod levels;
mod tree;

#[cfg(test)]
mod tests;

use anyhow::Result;
use colored::Colorize;

use crate::commands::common::find_work_dir;
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

    let work_dir = find_work_dir()?;

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
