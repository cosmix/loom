//! Topological level computation for stage graphs
//!
//! Re-exports the canonical compute_all_levels function.

pub use crate::commands::status::common::levels::compute_all_levels;

use crate::models::stage::Stage;
use std::collections::HashMap;

/// Compute the topological level for each stage.
/// Level = max(levels of all dependencies) + 1, with roots at level 0.
pub fn compute_stage_levels(stages: &[Stage]) -> HashMap<String, usize> {
    compute_all_levels(stages, |s| s.id.as_str(), |s| &s.dependencies)
}
