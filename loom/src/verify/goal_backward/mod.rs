//! Goal-backward verification system
//!
//! Validates OUTCOMES, not just task completion:
//! - TRUTHS: Observable behaviors that must work (shell commands return 0)
//! - ARTIFACTS: Files that must exist with actual implementation (not stubs)
//! - WIRING: Critical connections between components (grep patterns)

pub mod artifacts;
pub mod dead_code;
pub mod result;
pub mod truths;
pub mod wiring;
pub mod wiring_tests;

pub use artifacts::verify_artifacts;
pub use dead_code::run_dead_code_check;
pub use result::{GapType, GoalBackwardResult, VerificationGap};
pub use truths::{verify_truth_checks, verify_truths};
pub use wiring::verify_wiring;
pub use wiring_tests::verify_wiring_tests;

use crate::plan::schema::StageDefinition;
use anyhow::Result;
use std::path::Path;

/// Run complete goal-backward verification for a stage
pub fn run_goal_backward_verification(
    stage_def: &StageDefinition,
    working_dir: &Path,
) -> Result<GoalBackwardResult> {
    let mut gaps = Vec::new();

    // 1. Verify truths (observable behaviors - simple commands)
    if !stage_def.truths.is_empty() {
        gaps.extend(verify_truths(&stage_def.truths, working_dir)?);
    }

    // 2. Verify enhanced truth checks (commands with extended criteria)
    if !stage_def.truth_checks.is_empty() {
        gaps.extend(verify_truth_checks(&stage_def.truth_checks, working_dir)?);
    }

    // 3. Verify artifacts (files exist with implementation)
    if !stage_def.artifacts.is_empty() {
        gaps.extend(verify_artifacts(&stage_def.artifacts, working_dir)?);
    }

    // 4. Verify wiring (connections between components)
    if !stage_def.wiring.is_empty() {
        gaps.extend(verify_wiring(&stage_def.wiring, working_dir)?);
    }

    // 5. Verify wiring tests (command-based integration verification)
    if !stage_def.wiring_tests.is_empty() {
        gaps.extend(verify_wiring_tests(&stage_def.wiring_tests, working_dir)?);
    }

    // 6. Run dead code check if configured
    if let Some(dead_code_check) = &stage_def.dead_code_check {
        gaps.extend(run_dead_code_check(dead_code_check, working_dir)?);
    }

    Ok(GoalBackwardResult::from_gaps(gaps))
}
