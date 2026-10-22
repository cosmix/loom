//! Goal-backward verification command
//!
//! Validates OUTCOMES beyond acceptance criteria by checking:
//! - ARTIFACTS: Files exist with real implementation
//! - WIRING: Critical connections between components

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::collections::BTreeSet;
use std::path::Path;

use crate::commands::stage::acceptance_runner::{
    resolve_stage_execution_paths, run_selected_acceptance, AcceptanceDisplayOptions,
    StageExecutionPaths,
};
use crate::plan::parser::parse_plan;
use crate::verify::criteria::resolve_confinement;
use crate::verify::goal_backward::{run_goal_backward_verification, GoalBackwardResult};
use crate::verify::transitions::load_stage;

#[path = "verify_relay.rs"]
mod session_relay;

/// `loom check`'s run options.
#[derive(Debug, Clone, clap::Args)]
pub struct CheckArgs {
    /// Skip the acceptance-criterion pass cache lookup: run every criterion
    /// for real even if an identical tree already passed it. Passes are
    /// still stored, so a later `loom stage complete` can reuse them.
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Run only the acceptance criterion at this 0-based index, the number in
    /// its `[criterion <n>]` label (repeatable). A subset run skips the
    /// goal-backward checks.
    #[arg(long = "criterion", value_name = "INDEX")]
    pub criterion: Vec<usize>,
}

/// Execute the verify command. `criteria` limits the run to those 0-based
/// acceptance criteria; empty runs every criterion and the goal-backward
/// checks. The state directory resolves in the main repository and in a
/// worktree (where it is a symlink); outside a loom workspace this bails.
pub fn execute(stage_id: &str, suggest: bool, criteria: &[usize]) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;

    session_relay::run(stage_id, &work_dir, suggest, criteria)?;
    let stage = load_stage(stage_id, &work_dir)
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;

    // Resolve plan source path (handles both absolute and relative paths,
    // follows the state directory symlink in worktrees to find the main project root)
    let plan_path = crate::fs::resolve_source_path(&work_dir)?
        .context("No plan source path configured in config.toml")?;

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

    println!(
        "{} Running goal-backward verification for '{}'...\n",
        "→".cyan().bold(),
        stage_id
    );

    // 1. Run standard acceptance criteria first
    println!("{}", "Acceptance Criteria:".bold());
    let only = selection(criteria);
    let acceptance_passed = run_selected_acceptance(
        &stage,
        stage_id,
        &execution_paths,
        &work_dir,
        AcceptanceDisplayOptions::default(),
        only.as_ref(),
    )?;

    // 2. Run goal-backward verification
    let goal_passed = if let Some(only) = &only {
        print_subset_summary(only, acceptance_passed);
        true
    } else if stage_def.has_any_goal_checks() {
        run_goal_section(
            stage_id,
            &execution_paths,
            &work_dir,
            stage.plan_version,
            suggest,
            acceptance_passed,
        )?
    } else {
        print_no_goal_checks(acceptance_passed);
        true
    };

    ensure_passed(acceptance_passed, goal_passed)
}

/// The criteria a `--criterion` list selects; `None` when it names none.
fn selection(criteria: &[usize]) -> Option<BTreeSet<usize>> {
    (!criteria.is_empty()).then(|| criteria.iter().copied().collect())
}

/// Close a subset run: the goal-backward checks belong to a full run.
fn print_subset_summary(only: &BTreeSet<usize>, acceptance_passed: bool) {
    println!("\n{}", "Goal-Backward Verification:".dimmed());
    println!("  {} Skipped in a --criterion run", "−".dimmed());
    let indices: Vec<String> = only.iter().map(usize::to_string).collect();
    let mark = if acceptance_passed { "✓" } else { "✗" };
    println!(
        "\n{} subset: {} | Acceptance: {mark}",
        "Summary:".bold(),
        indices.join(", ")
    );
}

/// Turn the gate outcomes into the command's exit status: callers (stage agents,
/// scripts) read the exit code, so a failed gate must not exit 0.
fn ensure_passed(acceptance_passed: bool, goal_passed: bool) -> Result<()> {
    if acceptance_passed && goal_passed {
        return Ok(());
    }
    let failed: Vec<&str> = [
        (!acceptance_passed).then_some("acceptance"),
        (!goal_passed).then_some("goal-backward verification"),
    ]
    .into_iter()
    .flatten()
    .collect();
    bail!("Verification failed: {}", failed.join(" and "))
}

/// Report a stage that defines no artifacts or wiring.
fn print_no_goal_checks(acceptance_passed: bool) {
    println!("\n{}", "Goal-Backward Verification:".dimmed());
    println!("  {} No artifacts or wiring defined", "−".dimmed());

    if acceptance_passed {
        println!("\n{} Acceptance criteria passed!", "✓".green().bold());
    }
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

/// Run goal-backward verification and print its result and the summary.
fn run_goal_section(
    stage_id: &str,
    execution_paths: &StageExecutionPaths,
    work_dir: &Path,
    plan_version: u32,
    suggest: bool,
    acceptance_passed: bool,
) -> Result<bool> {
    println!("\n{}", "Goal-Backward Verification:".bold());

    let verify_dir = execution_paths
        .acceptance_dir
        .as_deref()
        .context("No working directory available for goal-backward verification")?;

    let goal_result = run_and_verify_stage_goal(
        stage_id,
        verify_dir,
        execution_paths.worktree_root.as_deref(),
        work_dir,
        plan_version,
    )?;
    print_goal_result(&goal_result, suggest);
    print_summary(acceptance_passed, goal_result.is_passed());
    Ok(goal_result.is_passed())
}

/// Print the final acceptance and goal-backward summary
fn print_summary(acceptance_passed: bool, goal_passed: bool) {
    println!();
    if acceptance_passed && goal_passed {
        println!("{} All verifications passed!", "✓".green().bold());
        return;
    }
    let mark = |passed: bool| if passed { "✓" } else { "✗" };
    println!(
        "{} Acceptance: {} | Goal-backward: {}",
        "Summary:".bold(),
        mark(acceptance_passed),
        mark(goal_passed)
    );
}

/// Shared helper: Load plan, find stage definition, run goal-backward verification
///
/// This helper encapsulates the common pattern used across:
/// - `loom stage complete` (with verification)
/// - `loom check` (standalone verification command)
///
/// `tree` is the stage worktree root the plan-authored commands may use in full.
/// `plan_version` is the runtime stage's plan version, which selects the
/// wiring rules.
///
/// Returns Ok(result) on successful verification run, Err if plan/stage not found.
pub fn run_and_verify_stage_goal(
    stage_id: &str,
    verification_dir: &Path,
    tree: Option<&Path>,
    work_dir: &Path,
    plan_version: u32,
) -> Result<GoalBackwardResult> {
    let plan_path = crate::fs::resolve_source_path(work_dir)?
        .context("No plan source path configured in config.toml")?;

    // Parse plan to get stage definition
    let plan = parse_plan(&plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;

    let stage_def = plan
        .stages
        .iter()
        .find(|s| s.id == stage_id)
        .with_context(|| format!("Stage '{stage_id}' not found in plan"))?;

    // Goal checks are plan-authored commands: run them at the stage's resolved
    // confinement level (stage override over plan default).
    let confinement = resolve_confinement(
        stage_def.sandbox.command_confinement,
        Some(plan.metadata.loom.sandbox.command_confinement),
    );

    // Run goal-backward verification
    run_goal_backward_verification(stage_def, verification_dir, tree, confinement, plan_version)
}

#[cfg(test)]
mod tests {
    use super::ensure_passed;

    #[test]
    fn passing_gates_exit_ok() {
        assert!(ensure_passed(true, true).is_ok());
    }

    #[test]
    fn failing_acceptance_is_an_error() {
        let err = ensure_passed(false, true).unwrap_err().to_string();
        assert!(err.contains("acceptance"), "{err}");
    }

    #[test]
    fn failing_goal_backward_is_an_error() {
        let err = ensure_passed(true, false).unwrap_err().to_string();
        assert!(err.contains("goal-backward"), "{err}");
    }
}
