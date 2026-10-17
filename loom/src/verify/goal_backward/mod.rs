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
use crate::models::dispute::CriterionField;
use crate::plan::schema::{CommandConfinement, StageDefinition};
use anyhow::{Context, Result};
use std::path::Path;

/// Run complete goal-backward verification for a stage
///
/// `tree` is the stage worktree root those commands may use in full (`None`:
/// `working_dir` is the tree). `confinement` is the stage's resolved level for
/// the plan-authored commands this runs (truth checks, wiring tests, dead-code checks). `plan_version`
/// is the plan's `loom.version`, which selects the wiring rules.
pub fn run_goal_backward_verification(
    stage_def: &StageDefinition,
    working_dir: &Path,
    tree: Option<&Path>,
    confinement: CommandConfinement,
    plan_version: u32,
) -> Result<GoalBackwardResult> {
    let mut gaps = Vec::new();

    // 1. Verify artifacts (files exist with implementation)
    if !stage_def.artifacts.is_empty() {
        gaps.extend(verify_artifacts(&stage_def.artifacts, working_dir)?);
    }

    // 2-3. Verify wiring checks (connections between components) and wiring
    // tests (command-based integration verification), each gap labelled with
    // its entry's index
    gaps.extend(wiring_gaps(
        stage_def,
        working_dir,
        tree,
        confinement,
        plan_version,
    )?);

    // 4. Run dead code check if configured
    if let Some(dead_code_check) = &stage_def.dead_code_check {
        gaps.extend(run_dead_code_check(
            dead_code_check,
            working_dir,
            tree,
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

/// The stage's wiring checks and wiring tests, verified one entry at a time so
/// each gap's description starts `[wiring <n>]` or `[wiring_tests <n>]`: the
/// 0-based index `loom stage dispute-criteria --field` takes.
fn wiring_gaps(
    stage_def: &StageDefinition,
    working_dir: &Path,
    tree: Option<&Path>,
    confinement: CommandConfinement,
    plan_version: u32,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();
    for (index, check) in stage_def.wiring.iter().enumerate() {
        let found = verify_wiring(std::slice::from_ref(check), working_dir, plan_version)?;
        gaps.extend(indexed_gaps(CriterionField::Wiring, index, found));
    }
    for (index, test) in stage_def.wiring_tests.iter().enumerate() {
        let found =
            verify_wiring_tests(std::slice::from_ref(test), working_dir, tree, confinement)?;
        gaps.extend(indexed_gaps(CriterionField::WiringTests, index, found));
    }
    Ok(gaps)
}

/// `gaps` of entry `index` of `field`'s list, each description prefixed with
/// `[<label> <index>]`.
fn indexed_gaps(
    field: CriterionField,
    index: usize,
    gaps: Vec<VerificationGap>,
) -> Vec<VerificationGap> {
    gaps.into_iter()
        .map(|mut gap| {
            gap.description = format!("[{} {index}] {}", field.gap_label(), gap.description);
            gap
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::{SuccessCriteria, WiringCheck, WiringTest};

    fn check(pattern: &str) -> WiringCheck {
        WiringCheck {
            source: "lib.rs".to_string(),
            pattern: pattern.to_string(),
            description: format!("{pattern} is wired"),
            literal: false,
        }
    }

    #[test]
    fn gaps_name_their_wiring_and_wiring_tests_index() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("lib.rs"), "fn main() { start(); }\n").unwrap();
        let stage_def = StageDefinition {
            wiring: vec![check("start\\(\\)"), check("never_called")],
            wiring_tests: vec![WiringTest {
                name: "always fails".to_string(),
                command: "false".to_string(),
                success_criteria: SuccessCriteria::default(),
                description: None,
            }],
            ..StageDefinition::default()
        };

        let result = run_goal_backward_verification(
            &stage_def,
            tmp.path(),
            None,
            CommandConfinement::default(),
            1,
        )
        .unwrap();

        let descriptions: Vec<&str> = result
            .gaps()
            .iter()
            .map(|gap| gap.description.as_str())
            .collect();
        assert_eq!(descriptions.len(), 2, "{descriptions:?}");
        assert!(
            descriptions[0].starts_with("[wiring 1] "),
            "{descriptions:?}"
        );
        assert!(descriptions[0].contains("never_called"), "{descriptions:?}");
        assert!(
            descriptions[1].starts_with("[wiring_tests 0] "),
            "{descriptions:?}"
        );
    }
}
