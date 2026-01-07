use crate::fs::work_dir::WorkDir;
use crate::models::stage::StageStatus;
use crate::verify::transitions::load_stage;
use crate::verify::{
    human_gate, run_acceptance, transition_stage, trigger_dependents, AcceptanceResult,
    GateConfig, GateDecision,
};
use anyhow::{bail, Context, Result};

/// Run acceptance criteria and prompt for human approval
/// Usage: loom verify <stage_id>
pub fn execute(stage_id: String) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    println!("Verifying stage: {stage_id}");
    println!();

    let stage = load_stage(&stage_id, work_dir.root())
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;

    if stage.status != StageStatus::Completed {
        bail!(
            "Stage '{stage_id}' is not in Completed status (current: {:?}). Only completed stages can be verified.",
            stage.status
        );
    }

    println!("Running acceptance criteria...");
    println!();

    let acceptance_result = run_acceptance(&stage)
        .with_context(|| format!("Failed to run acceptance criteria for stage '{stage_id}'"))?;

    display_acceptance_results(&acceptance_result);

    if !acceptance_result.all_passed() {
        println!();
        println!("Verification FAILED: Not all acceptance criteria passed.");
        println!();
        println!("Failed criteria:");
        for failure in acceptance_result.failures() {
            println!("  - {failure}");
        }
        bail!("Verification failed due to failing acceptance criteria");
    }

    println!();
    println!("All acceptance criteria passed!");

    let config = GateConfig::new();
    let decision = human_gate(&stage, &config)
        .context("Failed to execute human approval gate")?;

    match decision {
        GateDecision::Approved => {
            println!();
            println!("Stage approved! Transitioning to Verified status...");

            transition_stage(&stage_id, StageStatus::Verified, work_dir.root())
                .with_context(|| format!("Failed to transition stage '{stage_id}' to Verified"))?;

            let triggered = trigger_dependents(&stage_id, work_dir.root())
                .context("Failed to trigger dependent stages")?;

            if triggered.is_empty() {
                println!("No dependent stages were triggered.");
            } else {
                println!();
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  - {dep_id}");
                }
            }

            println!();
            println!("Stage '{stage_id}' successfully verified!");
        }
        GateDecision::Rejected { reason } => {
            println!();
            println!("Stage verification REJECTED.");
            println!("Reason: {reason}");
            println!();
            println!("Stage '{stage_id}' remains in Completed status.");
            bail!("Stage verification rejected by reviewer");
        }
        GateDecision::Skipped => {
            println!();
            println!("Verification skipped (auto-approve mode).");
        }
    }

    Ok(())
}

fn display_acceptance_results(result: &AcceptanceResult) {
    let total = result.results().len();
    let passed = result.passed_count();
    let failed = result.failed_count();

    println!("Acceptance Results:");
    println!("  Total:  {total}");
    println!("  Passed: {passed}");
    println!("  Failed: {failed}");
    println!();

    for criterion_result in result.results() {
        println!("  {}", criterion_result.summary());
    }
}
