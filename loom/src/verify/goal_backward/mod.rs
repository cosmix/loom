//! Goal-backward verification system
//!
//! Validates OUTCOMES, not just task completion:
//! - TRUTHS: Observable behaviors that must work (shell commands return 0)
//! - ARTIFACTS: Files that must exist with actual implementation (not stubs)
//! - WIRING: Critical connections between components (grep patterns)

pub mod artifacts;
pub mod result;
pub mod truths;
pub mod wiring;

pub use artifacts::verify_artifacts;
pub use result::{GapType, GoalBackwardResult, VerificationGap};
pub use truths::verify_truths;
pub use wiring::verify_wiring;

use crate::plan::schema::StageDefinition;
use anyhow::Result;
use std::path::Path;

/// Run complete goal-backward verification for a stage
pub fn run_goal_backward_verification(
    stage_def: &StageDefinition,
    working_dir: &Path,
) -> Result<GoalBackwardResult> {
    let mut gaps = Vec::new();

    // 1. Verify truths (observable behaviors)
    if !stage_def.truths.is_empty() {
        gaps.extend(verify_truths(&stage_def.truths, working_dir)?);
    }

    // 2. Verify artifacts (files exist with implementation)
    if !stage_def.artifacts.is_empty() {
        gaps.extend(verify_artifacts(&stage_def.artifacts, working_dir)?);
    }

    // 3. Verify wiring (connections between components)
    if !stage_def.wiring.is_empty() {
        gaps.extend(verify_wiring(&stage_def.wiring, working_dir)?);
    }

    Ok(GoalBackwardResult::from_gaps(gaps))
}
