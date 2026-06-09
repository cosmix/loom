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
pub mod tree;

#[cfg(test)]
mod tests;

use anyhow::Result;

use crate::commands::common::find_work_dir;
use crate::models::stage::StageStatus;
use crate::verify::transitions::list_all_stages;

// Re-export the public API
pub use colors::stage_color;
pub use display::build_graph_display;
pub use indicators::{status_indicator, status_priority};
pub use levels::compute_stage_levels;
pub use tree::build_tree_display;

/// All variants in display order for legend generation.
///
/// Ordered by operational significance (active → terminal → blocked).
const ALL_STATUSES: &[StageStatus] = &[
    StageStatus::Completed,
    StageStatus::Executing,
    StageStatus::Queued,
    StageStatus::WaitingForDeps,
    StageStatus::WaitingForInput,
    StageStatus::Blocked,
    StageStatus::NeedsHandoff,
    StageStatus::Skipped,
    StageStatus::MergeConflict,
    StageStatus::CompletedWithFailures,
    StageStatus::MergeBlocked,
    StageStatus::NeedsHumanReview,
    StageStatus::NeedsAdjudication,
];

/// Show the execution graph
pub fn show() -> Result<()> {
    crate::utils::print_logo_header("Execution Graph");

    let work_dir = find_work_dir()?;

    let stages = list_all_stages(&work_dir)?;
    let tree_display = build_tree_display(&stages);
    println!("{tree_display}");

    // Print legend generated from the canonical StageStatus methods so no
    // variant is ever omitted and icons/colors stay in sync automatically.
    println!();
    print!("Legend: ");
    for (i, status) in ALL_STATUSES.iter().enumerate() {
        if i > 0 {
            print!("  ");
        }
        print!(
            "{} {}",
            indicators::status_indicator(status),
            status.label()
        );
    }
    println!();
    println!();

    Ok(())
}
