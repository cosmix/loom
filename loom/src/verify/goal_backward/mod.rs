//! Goal-backward verification system
//!
//! Validates OUTCOMES, not just task completion:
//! - ARTIFACTS: Files that must exist with actual implementation (not stubs)
//! - WIRING: Critical connections between components (grep patterns)
//! - REACHABLE: Units the source graph must reach from an entry point (plan v2)

pub mod artifacts;
pub mod dead_code;
mod definition_sites;
pub mod reachable;
pub mod result;
pub mod truths;
pub mod wiring;
pub mod wiring_tests;
mod wiring_v2;

pub use artifacts::{verify_artifacts, verify_regression_test};
pub use dead_code::run_dead_code_check;
pub use result::{GapType, GoalBackwardResult, VerificationGap};
pub use truths::verify_truth_checks;
pub use wiring::verify_wiring;
pub use wiring_tests::verify_wiring_tests;

use crate::context::worktree_graph::build_for_worktree;
use crate::plan::schema::{CommandConfinement, StageDefinition};
use anyhow::{Context, Result};
use std::path::Path;

/// Run complete goal-backward verification for a stage
///
/// `confinement` is the stage's resolved level for the plan-authored commands
/// this runs (truth checks, wiring tests, dead-code checks). `plan_version`
/// is the plan's `loom.version`, which selects the wiring rules.
pub fn run_goal_backward_verification(
    stage_def: &StageDefinition,
    working_dir: &Path,
    confinement: CommandConfinement,
    plan_version: u32,
) -> Result<GoalBackwardResult> {
    let mut gaps = Vec::new();

    // 1. Verify artifacts (files exist with implementation)
    if !stage_def.artifacts.is_empty() {
        gaps.extend(verify_artifacts(&stage_def.artifacts, working_dir)?);
    }

    // 2. Verify wiring (connections between components)
    if !stage_def.wiring.is_empty() {
        gaps.extend(verify_wiring(&stage_def.wiring, working_dir, plan_version)?);
    }

    // 3. Verify wiring tests (command-based integration verification)
    if !stage_def.wiring_tests.is_empty() {
        gaps.extend(verify_wiring_tests(
            &stage_def.wiring_tests,
            working_dir,
            confinement,
        )?);
    }

    // 4. Run dead code check if configured
    if let Some(dead_code_check) = &stage_def.dead_code_check {
        gaps.extend(run_dead_code_check(
            dead_code_check,
            working_dir,
            confinement,
        )?);
    }

    // 5. Verify regression test (for bug-fix stages)
    if let Some(ref regression_test) = stage_def.regression_test {
        gaps.extend(artifacts::verify_regression_test(
            regression_test,
            working_dir,
        )?);
    }

    // 6. Verify reachable units (plan version 2)
    gaps.extend(reachable_gaps(stage_def, working_dir, plan_version)?);

    Ok(GoalBackwardResult::from_gaps(gaps))
}

/// Run a v2 stage's `reachable` checks against one worktree graph, built once
/// for all of them. A graph that cannot be built is an error, never a pass.
fn reachable_gaps(
    stage_def: &StageDefinition,
    working_dir: &Path,
    plan_version: u32,
) -> Result<Vec<VerificationGap>> {
    if plan_version != 2 || stage_def.reachable.is_empty() {
        return Ok(Vec::new());
    }
    let graph = build_for_worktree(working_dir)
        .context("building the worktree source graph for reachable checks")?;
    Ok(reachable::verify_reachable(&stage_def.reachable, &graph))
}
