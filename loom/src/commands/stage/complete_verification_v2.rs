//! The verification v2 checks at stage completion, each gated on its stage
//! type: the contract check (DESIGN D9), the test-integrity gate (D13),
//! impact-selected tests (D14), the integration-verify reachable
//! re-verification (D11) and the recorded review gate (D12). Later v2 checks
//! belong here too.

use super::{completed_definitions, CheckSink, VerificationChecks, IMPACT_TESTS_LABEL};
use crate::context::worktree_graph::build_for_worktree;
use crate::models::stage::StageType;
use crate::plan::schema::StageDefinition;
use crate::verify::criteria::plan_confinement;
use crate::verify::goal_backward::reachable::verify_reachable;
use crate::verify::{contracts::completion, impact_tests, integrity, review::gate};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Run every v2 check, cheap deterministic failures first, handing the
/// contract check, the impact-selected tests and the reachable
/// re-verification to `sink` under their labels. The review gate runs last:
/// every edit made to fix an earlier failure needs a re-review.
///
/// The test-integrity and review gates compare this worktree's change
/// fingerprint with values the loom daemon recorded, so the daemon computes
/// it (`fingerprint::compute`). With `daemon_gates` (the daemon's completion
/// job) they are not run here: the daemon runs both when it applies the
/// job's result, before it completes the stage.
pub(super) fn run_v2(
    checks: &VerificationChecks<'_>,
    target_branch: &str,
    sink: &mut CheckSink<'_>,
) -> Result<()> {
    let (stage, work_dir) = (checks.stage, checks.work_dir);
    let standard = stage.stage_type == StageType::Standard;
    let integration = stage.stage_type == StageType::IntegrationVerify;
    if standard && !stage.contracts.is_empty() {
        sink("contracts", check_contracts(checks))?;
    }
    if (standard || integration) && !checks.daemon_gates {
        let worktree_root = worktree(checks, "check test integrity in")?;
        integrity::check(stage, work_dir, worktree_root, target_branch)?;
    }
    if standard {
        sink(IMPACT_TESTS_LABEL, run_impact_tests(checks, target_branch))?;
    }
    if integration {
        sink("reachable", reverify_reachable(checks))?;
    }
    if checks.daemon_gates {
        if gate::covers(stage) {
            println!(
                "Test integrity and the review gate are checked by the loom daemon when it \
                 applies this completion."
            );
        }
        return Ok(());
    }
    gate::check_at_completion(stage, work_dir, checks.worktree_root, target_branch)
}

/// DESIGN D9: every frozen contract still passes on the stage's tree.
fn check_contracts(checks: &VerificationChecks<'_>) -> Result<()> {
    let worktree_root = worktree(checks, "check contracts in")?;
    let acceptance_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
    completion::check(checks.stage, checks.work_dir, acceptance_dir, worktree_root)
}

/// The stage's worktree, which every check reading the tree needs.
fn worktree<'a>(checks: &VerificationChecks<'a>, purpose: &str) -> Result<&'a Path> {
    checks
        .worktree_root
        .with_context(|| format!("Stage '{}': no worktree to {purpose}", checks.stage.id))
}

/// DESIGN D14: the tests that reach what the stage changed since its merge
/// base with `target_branch` pass. A test that cannot be selected or run, one
/// whose tests fail at the base with at least as many failures counted, and
/// one a dispute accepted are notes, never failures. A failure is
/// `impact_tests::ImpactTestsFailed`, which `complete` records for a dispute.
fn run_impact_tests(checks: &VerificationChecks<'_>, target_branch: &str) -> Result<()> {
    println!("Running impact-selected tests...");
    let working_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
    let work_dir = checks.work_dir;
    let config =
        impact_tests::criteria_config(work_dir, plan_confinement(work_dir), checks.worktree_root);
    let outcome = impact_tests::run(checks.stage, work_dir, working_dir, &config, target_branch)?;
    for note in &outcome.notes {
        println!("  Note: {note}");
    }
    for ran in &outcome.ran {
        println!("  ✓ {ran}");
    }
    Ok(())
}

/// DESIGN D11: every completed stage's `reachable` checks still hold on the
/// merged tree. One graph serves every stage and is built from the worktree
/// root: joining a stage's `working_dir` onto the already-resolved
/// `acceptance_dir` would apply a `working_dir` twice.
fn reverify_reachable(checks: &VerificationChecks<'_>) -> Result<()> {
    let definitions: Vec<StageDefinition> = completed_definitions(checks.work_dir)?
        .into_iter()
        .filter(|definition| !definition.reachable.is_empty())
        .collect();
    if definitions.is_empty() {
        return Ok(());
    }
    println!("Running aggregated reachable re-verification...");
    let worktree_root = worktree(checks, "re-verify reachable units in")?;
    let graph = build_for_worktree(worktree_root)
        .context("building the worktree source graph for reachable re-verification")?;
    let mut failures = Vec::new();
    for definition in &definitions {
        for gap in verify_reachable(&definition.reachable, &graph) {
            failures.push(format!(
                "stage '{}': {}\n    → {}",
                definition.id, gap.description, gap.suggestion
            ));
        }
    }
    if failures.is_empty() {
        println!("Aggregated reachable re-verification passed!");
        return Ok(());
    }
    bail!(
        "Aggregated reachable re-verification failed on the merged tree:\n  - {}",
        failures.join("\n  - ")
    )
}

#[cfg(test)]
#[path = "complete_verification_v2_tests.rs"]
mod tests;
