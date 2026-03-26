//! Goal-backward verification command
//!
//! Validates OUTCOMES beyond acceptance criteria by checking:
//! - ARTIFACTS: Files exist with real implementation
//! - WIRING: Critical connections between components

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

use crate::commands::stage::acceptance_runner::{
    resolve_stage_execution_paths, run_acceptance_with_display, AcceptanceDisplayOptions,
};
use crate::fs::work_dir::load_config_required;
use crate::plan::parser::parse_plan;
use crate::plan::schema::StageDefinition;
use crate::verify::goal_backward::{run_goal_backward_verification, GoalBackwardResult};
use crate::verify::transitions::load_stage;

/// Execute the verify command
pub fn execute(stage_id: &str, suggest: bool) -> Result<()> {
    // Use .work directly (works in main repo and worktrees with symlink)
    let work_dir = Path::new(".work");
    if !work_dir.exists() {
        anyhow::bail!(".work directory does not exist. Run 'loom init' first.");
    }

    // Load stage
    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;

    // Get plan source path
    let config = load_config_required(work_dir)?;
    let plan_path = config
        .source_path()
        .context("No plan source path configured in .work/config.toml")?;

    // Parse plan to get stage definition
    let plan = parse_plan(&plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;

    let stage_def = plan
        .stages
        .iter()
        .find(|s| s.id == stage_id)
        .with_context(|| format!("Stage '{stage_id}' not found in plan"))?;

    // Resolve acceptance directory using shared stage execution logic
    let execution_paths = resolve_stage_execution_paths(&stage)?;
    let acceptance_dir = execution_paths.acceptance_dir;

    println!(
        "{} Running goal-backward verification for '{}'...\n",
        "→".cyan().bold(),
        stage_id
    );

    // 1. Run standard acceptance criteria first
    println!("{}", "Acceptance Criteria:".bold());
    let acceptance_passed = run_acceptance_with_display(
        &stage,
        stage_id,
        acceptance_dir.as_deref(),
        AcceptanceDisplayOptions {
            stage_label: None,
            show_empty_message: false,
        },
    )?;

    // 2. Run goal-backward verification
    if stage_def.has_any_goal_checks() {
        println!("\n{}", "Goal-Backward Verification:".bold());

        // Use acceptance_dir or fall back to current directory
        let verify_dir = acceptance_dir
            .as_deref()
            .context("No working directory available for goal-backward verification")?;

        // Use shared helper for verification
        let goal_result = run_and_verify_stage_goal(stage_id, verify_dir, work_dir)?;
        print_goal_result(&goal_result, suggest);

        // Final summary
        println!();
        if acceptance_passed && goal_result.is_passed() {
            println!("{} All verifications passed!", "✓".green().bold());
        } else {
            let acceptance_ok = if acceptance_passed { "✓" } else { "✗" };
            let goal_ok = if goal_result.is_passed() {
                "✓"
            } else {
                "✗"
            };
            println!(
                "{} Acceptance: {} | Goal-backward: {}",
                "Summary:".bold(),
                acceptance_ok,
                goal_ok
            );
        }
    } else {
        println!("\n{}", "Goal-Backward Verification:".dimmed());
        println!("  {} No artifacts or wiring defined", "−".dimmed());

        if acceptance_passed {
            println!("\n{} Acceptance criteria passed!", "✓".green().bold());
        }
    }

    Ok(())
}

/// Print goal-backward verification results
fn print_goal_result(result: &GoalBackwardResult, suggest: bool) {
    match result {
        GoalBackwardResult::Passed => {
            println!("  {} All artifacts exist", "✓".green());
            println!("  {} All wiring connected", "✓".green());
        }
        GoalBackwardResult::GapsFound { gaps } => {
            println!("  {} Found {} gap(s):", "✗".red(), gaps.len());
            for gap in gaps {
                println!("    {} {}", "→".yellow(), gap.description);
                if suggest {
                    println!("      {} {}", "Fix:".dimmed(), gap.suggestion);
                }
            }
        }
        GoalBackwardResult::HumanNeeded { checks } => {
            println!("  {} Manual review needed:", "?".yellow());
            for check in checks {
                println!("    {check}");
            }
        }
    }
}

/// Shared helper: Load plan, find stage definition, run goal-backward verification
///
/// This helper encapsulates the common pattern used across:
/// - `loom stage complete` (with verification)
/// - `loom stage verify` (re-verification after fixes)
/// - `loom verify` (standalone verification command)
///
/// Returns Ok(result) on successful verification run, Err if plan/stage not found.
pub fn run_and_verify_stage_goal(
    stage_id: &str,
    verification_dir: &Path,
    work_dir: &Path,
) -> Result<GoalBackwardResult> {
    // Load config to get plan source path
    let config = load_config_required(work_dir)?;
    let plan_path = config
        .source_path()
        .context("No plan source path configured in .work/config.toml")?;

    // Parse plan to get stage definition
    let plan = parse_plan(&plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;

    let stage_def = plan
        .stages
        .iter()
        .find(|s| s.id == stage_id)
        .with_context(|| format!("Stage '{stage_id}' not found in plan"))?;

    // Run goal-backward verification
    run_goal_backward_verification(stage_def, verification_dir)
}

/// Load stage definition from the active plan
///
/// Used by callers that need the stage definition for other purposes
/// (e.g., checking has_any_goal_checks() before calling verification).
pub fn load_stage_definition_from_plan(
    stage_id: &str,
    work_dir: &Path,
) -> Result<Option<StageDefinition>> {
    let config = match crate::fs::work_dir::load_config(work_dir)? {
        Some(config) => config,
        None => return Ok(None),
    };

    let plan_path = match config.source_path() {
        Some(path) => path,
        None => return Ok(None),
    };

    let plan = parse_plan(&plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;

    Ok(plan.stages.into_iter().find(|s| s.id == stage_id))
}
