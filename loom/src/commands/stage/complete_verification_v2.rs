//! The verification v2 checks at stage completion, each gated on its stage
//! type: the contract check (DESIGN D9), the test-integrity gate (D13),
//! impact-selected tests (D14), the integration-verify reachable
//! re-verification (D11) and the recorded review gate (D12). Later v2 checks
//! belong here too.

use super::{completed_definitions, VerificationChecks};
use crate::context::worktree_graph::build_for_worktree;
use crate::models::stage::StageType;
use crate::plan::schema::StageDefinition;
use crate::verify::criteria::{plan_confinement, CriteriaConfig};
use crate::verify::goal_backward::reachable::verify_reachable;
use crate::verify::{contracts::completion, impact_tests, integrity, review::gate};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Run every v2 check, cheap deterministic failures first. The review gate
/// runs last: every edit made to fix an earlier failure needs a re-review.
///
/// The test-integrity and review gates compare this worktree's change
/// fingerprint with values the loom daemon recorded, so the daemon computes
/// it (`fingerprint::compute`). A sandboxed stage session (`control_session`)
/// cannot reach the daemon; its completion reaches the daemon through the
/// broker instead, and the daemon runs both gates before it applies the
/// transition, so they are not run here.
pub(super) fn run_v2(checks: &VerificationChecks<'_>, target_branch: &str) -> Result<()> {
    let (stage, work_dir) = (checks.stage, checks.work_dir);
    let standard = stage.stage_type == StageType::Standard;
    let integration = stage.stage_type == StageType::IntegrationVerify;
    let daemon_gates = checks.control_session.is_some();
    if standard && !stage.contracts.is_empty() {
        let worktree_root = worktree(checks, "check contracts in")?;
        let acceptance_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
        completion::check(stage, work_dir, acceptance_dir, worktree_root)?;
    }
    if (standard || integration) && !daemon_gates {
        let worktree_root = worktree(checks, "check test integrity in")?;
        integrity::check(stage, work_dir, worktree_root, target_branch)?;
    }
    if standard {
        run_impact_tests(checks)?;
    }
    if integration {
        reverify_reachable(checks)?;
    }
    if daemon_gates {
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

/// The stage's worktree, which every check reading the tree needs.
fn worktree<'a>(checks: &VerificationChecks<'a>, purpose: &str) -> Result<&'a Path> {
    checks
        .worktree_root
        .with_context(|| format!("Stage '{}': no worktree to {purpose}", checks.stage.id))
}

/// DESIGN D14: the tests that reach the stage's changes pass. A test that
/// cannot be selected or run is a note, never a failure.
fn run_impact_tests(checks: &VerificationChecks<'_>) -> Result<()> {
    println!("Running impact-selected tests...");
    let working_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
    let config = CriteriaConfig::default()
        .with_plan_confinement(plan_confinement(checks.work_dir))
        .with_cache_dir(checks.work_dir);
    let outcome = impact_tests::run(checks.stage, working_dir, &config)?;
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
