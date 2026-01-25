//! Goal-backward verification command
//!
//! Validates OUTCOMES beyond acceptance criteria by checking:
//! - TRUTHS: Observable behaviors (shell commands return 0)
//! - ARTIFACTS: Files exist with real implementation
//! - WIRING: Critical connections between components

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;

use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;
use crate::plan::parser::parse_plan;
use crate::verify::criteria::run_acceptance;
use crate::verify::goal_backward::{run_goal_backward_verification, GoalBackwardResult};
use crate::verify::transitions::load_stage;

/// Execute the verify command
pub fn execute(stage_id: &str, suggest: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // Load stage
    let stage = load_stage(stage_id, work_dir.root())
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;

    // Get plan source path
    let config = work_dir.load_config_required()?;
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

    // Resolve acceptance directory
    let acceptance_dir = resolve_acceptance_dir_for_stage(&stage, &work_dir)?;

    println!(
        "{} Running goal-backward verification for '{}'...\n",
        "→".cyan().bold(),
        stage_id
    );

    // 1. Run standard acceptance criteria first
    println!("{}", "Acceptance Criteria:".bold());
    let acceptance_result = run_acceptance(&stage, acceptance_dir.as_deref())?;
    print_acceptance_result(&acceptance_result);

    // 2. Run goal-backward verification
    let has_goal_checks = !stage_def.truths.is_empty()
        || !stage_def.artifacts.is_empty()
        || !stage_def.wiring.is_empty();

    if has_goal_checks {
        println!("\n{}", "Goal-Backward Verification:".bold());

        // Use acceptance_dir or fall back to current directory
        let verify_dir = acceptance_dir
            .as_deref()
            .context("No working directory available for goal-backward verification")?;

        let goal_result = run_goal_backward_verification(stage_def, verify_dir)?;
        print_goal_result(&goal_result, suggest);

        // Final summary
        println!();
        if acceptance_result.all_passed() && goal_result.is_passed() {
            println!("{} All verifications passed!", "✓".green().bold());
        } else {
            let acceptance_ok = if acceptance_result.all_passed() {
                "✓"
            } else {
                "✗"
            };
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
        println!("  {} No truths, artifacts, or wiring defined", "−".dimmed());

        if acceptance_result.all_passed() {
            println!("\n{} Acceptance criteria passed!", "✓".green().bold());
        }
    }

    Ok(())
}

/// Resolve the directory for running acceptance criteria
fn resolve_acceptance_dir_for_stage(stage: &Stage, work_dir: &WorkDir) -> Result<Option<PathBuf>> {
    // Check if stage has a worktree
    if let Some(worktree) = &stage.worktree {
        let worktree_path = work_dir
            .root()
            .parent()
            .unwrap()
            .join(".worktrees")
            .join(worktree);
        if worktree_path.exists() {
            // Apply working_dir if set
            if let Some(wd) = &stage.working_dir {
                if wd != "." {
                    return Ok(Some(worktree_path.join(wd)));
                }
            }
            return Ok(Some(worktree_path));
        }
    }

    // Fall back to project root with working_dir
    if let Some(project_root) = work_dir.project_root() {
        let base = project_root.to_path_buf();
        if let Some(wd) = &stage.working_dir {
            if wd != "." {
                return Ok(Some(base.join(wd)));
            }
        }
        return Ok(Some(base));
    }

    Ok(None)
}

/// Print acceptance criteria results
fn print_acceptance_result(result: &crate::verify::criteria::AcceptanceResult) {
    for r in result.results() {
        let status = if r.success {
            "✓".green()
        } else {
            "✗".red()
        };
        println!("  {} {}", status, r.command);
    }
}

/// Print goal-backward verification results
fn print_goal_result(result: &GoalBackwardResult, suggest: bool) {
    match result {
        GoalBackwardResult::Passed => {
            println!("  {} All truths verified", "✓".green());
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
