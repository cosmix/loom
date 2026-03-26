//! Goal-backward verification system
//!
//! Validates OUTCOMES, not just task completion:
//! - ARTIFACTS: Files that must exist with actual implementation (not stubs)
//! - WIRING: Critical connections between components (grep patterns)

pub mod artifacts;
pub mod dead_code;
pub mod result;
pub mod truths;
pub mod wiring;
pub mod wiring_tests;

pub use artifacts::{verify_artifacts, verify_regression_test};
pub use dead_code::run_dead_code_check;
pub use result::{GapType, GoalBackwardResult, VerificationGap};
pub use truths::verify_truth_checks;
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

    // 1. Verify artifacts (files exist with implementation)
    if !stage_def.artifacts.is_empty() {
        gaps.extend(verify_artifacts(&stage_def.artifacts, working_dir)?);
    }

    // 2. Verify wiring (connections between components)
    if !stage_def.wiring.is_empty() {
        gaps.extend(verify_wiring(&stage_def.wiring, working_dir)?);
    }

    // 3. Verify wiring tests (command-based integration verification)
    if !stage_def.wiring_tests.is_empty() {
        gaps.extend(verify_wiring_tests(&stage_def.wiring_tests, working_dir)?);
    }

    // 4. Run dead code check if configured
    if let Some(dead_code_check) = &stage_def.dead_code_check {
        gaps.extend(run_dead_code_check(dead_code_check, working_dir)?);
    }

    // 5. Verify regression test (for bug-fix stages)
    if let Some(ref regression_test) = stage_def.regression_test {
        gaps.extend(artifacts::verify_regression_test(
            regression_test,
            working_dir,
        )?);
    }

    Ok(GoalBackwardResult::from_gaps(gaps))
}
