//! Verification-only checks used by the stage completion command.

use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::plan::parser::{load_stage_definition_from_plan, parse_plan, ParsedPlan};
use crate::plan::schema::{
    ChangeImpactConfig, ChangeImpactPolicy, CommandConfinement, StageDefinition,
};
use crate::verify::baseline::{compare_to_baseline, ChangeImpact};
use crate::verify::criteria::{plan_confinement, resolve_confinement};
use crate::verify::duplicate_detection::detect_duplicate_symbols;
use crate::verify::wiring_detection::{detect_unwired_files, UnwiredFile};
use anyhow::{bail, Context, Result};
use std::path::Path;

pub(super) struct VerificationChecks<'a> {
    pub stage: &'a Stage,
    pub stage_id: &'a str,
    pub acceptance_dir: Option<&'a Path>,
    pub worktree_root: Option<&'a Path>,
    pub control_session: Option<&'a str>,
    pub work_dir: &'a Path,
}

impl VerificationChecks<'_> {
    /// Confinement level for the plan-authored commands these checks run:
    /// the stage's own override, else the plan-level default, else `Confined`.
    fn command_confinement(&self) -> CommandConfinement {
        resolve_confinement(
            self.stage.sandbox.command_confinement,
            plan_confinement(self.work_dir),
        )
    }
}

pub(super) fn run(checks: &VerificationChecks<'_>) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or(cwd);
    let base_branch = crate::fs::resolve_target_branch_from_config(checks.work_dir, &repo_root)?;
    let stage_def = load_stage_definition_from_plan(checks.stage_id, checks.work_dir)?;

    run_goal_checks(checks, stage_def.as_ref())?;
    run_after_checks(checks, stage_def.as_ref())?;
    run_unwired_check(checks, &base_branch)?;
    run_duplicate_check(checks, &base_branch)?;
    run_aggregated_check(checks)?;
    run_change_impact(checks)?;
    Ok(())
}

fn run_goal_checks(
    checks: &VerificationChecks<'_>,
    stage_def: Option<&StageDefinition>,
) -> Result<()> {
    if !stage_def.is_some_and(StageDefinition::has_any_goal_checks) {
        return Ok(());
    }
    println!("Running goal-backward verification...");
    let verification_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
    let result = crate::commands::verify::run_and_verify_stage_goal(
        checks.stage_id,
        verification_dir,
        checks.work_dir,
    )?;
    if result.is_passed() {
        println!("Goal-backward verification passed!");
        return Ok(());
    }
    for gap in result.gaps() {
        eprintln!("  ✗ {:?}: {}", gap.gap_type, gap.description);
        eprintln!("    → {}", gap.suggestion);
    }
    bail!(
        "Goal-backward verification failed for stage '{}'",
        checks.stage_id
    )
}

fn run_after_checks(
    checks: &VerificationChecks<'_>,
    stage_def: Option<&StageDefinition>,
) -> Result<()> {
    let Some(stage_def) = stage_def.filter(|definition| !definition.after_stage.is_empty()) else {
        return Ok(());
    };
    println!("Running after-stage verification...");
    let verification_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
    let gaps = crate::verify::before_after::run_after_stage_checks(
        &stage_def.after_stage,
        verification_dir,
        checks.command_confinement(),
    )?;
    if gaps.is_empty() {
        println!("After-stage verification passed!");
        return Ok(());
    }
    for gap in &gaps {
        eprintln!("  ✗ After-stage: {}", gap.description);
        eprintln!("    → {}", gap.suggestion);
    }
    bail!(
        "After-stage verification failed for stage '{}'",
        checks.stage_id
    )
}

fn run_unwired_check(checks: &VerificationChecks<'_>, base_branch: &str) -> Result<()> {
    let Some(verification_dir) = checks.acceptance_dir else {
        return Ok(());
    };
    let result = match detect_unwired_files(verification_dir, base_branch) {
        Ok(result) => result,
        Err(error) => {
            fail_closed_or_warn(checks, "Wiring detection", error)?;
            return Ok(());
        }
    };
    if result.unwired_files.is_empty() {
        return Ok(());
    }
    if has_downstream_dependents(checks.stage_id, checks.work_dir) {
        verify_downstream_memory(checks, &result.unwired_files)
    } else {
        report_unwired_leaf(checks.stage_id, &result.unwired_files)
    }
}

fn verify_downstream_memory(checks: &VerificationChecks<'_>, files: &[UnwiredFile]) -> Result<()> {
    eprintln!("Warning: {} potentially unwired file(s):", files.len());
    report_unwired_files(files);
    if memory_covers_unwired(checks.stage_id, files, checks.work_dir) {
        println!("Unwired files found but memory notes cover downstream wiring plan.");
        return Ok(());
    }
    eprintln!("New files are not wired and no memory notes explain downstream wiring.");
    bail!("Unwired files detected with no memory notes for downstream wiring")
}

fn report_unwired_leaf(stage_id: &str, files: &[UnwiredFile]) -> Result<()> {
    eprintln!("ERROR: Unwired files in leaf stage (no downstream dependents):");
    report_unwired_files(files);
    bail!("Unwired files detected in leaf stage '{stage_id}'. Wire them or remove them.")
}

fn report_unwired_files(files: &[UnwiredFile]) {
    for file in files {
        eprintln!(
            "  - {} (importable as '{}')",
            file.path, file.importable_name
        );
    }
}

fn run_duplicate_check(checks: &VerificationChecks<'_>, base_branch: &str) -> Result<()> {
    let Some(verification_dir) = checks.acceptance_dir else {
        return Ok(());
    };
    let duplicates = match detect_duplicate_symbols(verification_dir, base_branch) {
        Ok(duplicates) => duplicates,
        Err(error) => return fail_closed_or_warn(checks, "Duplicate-symbol detection", error),
    };
    if !duplicates.is_empty() {
        println!("Potential duplicate symbols detected:");
    }
    for duplicate in duplicates {
        println!(
            "  Warning: New {} '{}' in {}:{} may duplicate existing '{}' in {}:{}",
            duplicate.symbol_type,
            duplicate.symbol_name,
            duplicate.new_file,
            duplicate.new_line,
            duplicate.symbol_name,
            duplicate.existing_file,
            duplicate.existing_line
        );
    }
    Ok(())
}

fn run_aggregated_check(checks: &VerificationChecks<'_>) -> Result<()> {
    if checks.stage.stage_type != StageType::IntegrationVerify {
        return Ok(());
    }
    let Some(worktree_root) = checks.worktree_root else {
        return Ok(());
    };
    println!("Running aggregated wiring re-verification...");
    aggregated_wiring(worktree_root, checks.work_dir)
}

fn aggregated_wiring(worktree_root: &Path, work_dir: &Path) -> Result<()> {
    let stages = crate::verify::transitions::list_all_stages(work_dir)?;
    let Some(plan) = load_parsed_plan(work_dir)? else {
        bail!("Could not load plan for aggregated wiring verification");
    };
    let mut all_gaps = Vec::new();
    for stage in stages
        .iter()
        .filter(|stage| stage.status == StageStatus::Completed)
    {
        let Some(definition) = plan
            .metadata
            .loom
            .stages
            .iter()
            .find(|item| item.id == stage.id)
        else {
            continue;
        };
        all_gaps.extend(stage_wiring_gaps(
            stage.id.as_str(),
            definition,
            worktree_root,
        )?);
    }
    if !all_gaps.is_empty() {
        bail!(
            "Aggregated wiring re-verification failed with {} issue(s)",
            all_gaps.len()
        );
    }
    println!("Aggregated wiring re-verification passed!");
    Ok(())
}

fn stage_wiring_gaps(
    stage_id: &str,
    definition: &StageDefinition,
    worktree_root: &Path,
) -> Result<Vec<crate::verify::goal_backward::VerificationGap>> {
    if definition.wiring.is_empty() {
        return Ok(Vec::new());
    }
    println!("  Re-verifying wiring from stage '{stage_id}'...");
    let working_dir = if definition.working_dir == "." {
        worktree_root.to_path_buf()
    } else {
        worktree_root.join(&definition.working_dir)
    };
    let gaps = crate::verify::goal_backward::verify_wiring(&definition.wiring, &working_dir)?;
    for gap in &gaps {
        eprintln!("    ✗ {stage_id}: {}", gap.description);
    }
    Ok(gaps)
}

fn run_change_impact(checks: &VerificationChecks<'_>) -> Result<()> {
    let Some(config) = load_change_impact_config(checks.work_dir)? else {
        return Ok(());
    };
    if config.policy == ChangeImpactPolicy::Skip {
        return Ok(());
    }
    println!("Running change impact comparison...");
    let impact = match compare_to_baseline(
        checks.stage_id,
        &config,
        checks.acceptance_dir,
        checks.work_dir,
        checks.command_confinement(),
    ) {
        Ok(impact) => impact,
        Err(error) => {
            fail_closed_or_warn(checks, "Change impact comparison", error)?;
            return Ok(());
        }
    };
    if !impact.comparison_succeeded {
        return fail_closed_or_warn(
            checks,
            "Change impact comparison",
            anyhow::anyhow!("comparison command did not complete successfully"),
        );
    }
    report_change_impact(&impact);
    enforce_change_impact(checks.stage_id, config.policy, &impact)
}

fn report_change_impact(impact: &ChangeImpact) {
    println!("  {}", impact.summary());
    if impact.has_new_failures() {
        println!("  New failures detected:");
        for failure in &impact.new_failures {
            println!("    - {failure}");
        }
    }
    if !impact.fixed_failures.is_empty() {
        println!("  Fixed failures:");
        for fixed in &impact.fixed_failures {
            println!("    + {fixed}");
        }
    }
}

fn enforce_change_impact(
    stage_id: &str,
    policy: ChangeImpactPolicy,
    impact: &ChangeImpact,
) -> Result<()> {
    if !impact.has_new_failures() {
        return Ok(());
    }
    match policy {
        ChangeImpactPolicy::Fail => {
            eprintln!(
                "Change impact check FAILED for stage '{stage_id}' - new failures introduced"
            );
            eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
            bail!("Change impact check failed for stage '{stage_id}' - new failures introduced");
        }
        ChangeImpactPolicy::Warn => {
            eprintln!("⚠️  Warning: New failures introduced, but continuing due to 'warn' policy")
        }
        ChangeImpactPolicy::Skip => {}
    }
    Ok(())
}

fn fail_closed_or_warn(
    checks: &VerificationChecks<'_>,
    label: &str,
    error: anyhow::Error,
) -> Result<()> {
    if checks.control_session.is_some() {
        bail!("{label} failed inside trusted completion verification: {error}");
    }
    eprintln!("Warning: {label} skipped: {error}");
    Ok(())
}

fn has_downstream_dependents(stage_id: &str, work_dir: &Path) -> bool {
    crate::verify::transitions::list_all_stages(work_dir)
        .map(|stages| {
            stages
                .iter()
                .any(|stage| stage.dependencies.iter().any(|id| id == stage_id))
        })
        .unwrap_or(false)
}

fn memory_covers_unwired(stage_id: &str, files: &[UnwiredFile], work_dir: &Path) -> bool {
    let path = work_dir.join("memory").join(format!("{stage_id}.md"));
    let Ok(content) = std::fs::read_to_string(path).map(|content| content.to_lowercase()) else {
        return false;
    };
    let has_context = [
        "wire",
        "wiring",
        "register",
        "mount",
        "import",
        "downstream",
        "integrate",
    ]
    .iter()
    .any(|keyword| content.contains(keyword));
    has_context
        || files.iter().any(|file| {
            content.contains(&file.importable_name.to_lowercase())
                || content.contains(&file.path.to_lowercase())
        })
}

fn load_parsed_plan(work_dir: &Path) -> Result<Option<ParsedPlan>> {
    let Some(plan_path) = crate::fs::resolve_source_path(work_dir)? else {
        return Ok(None);
    };
    parse_plan(&plan_path).map(Some)
}

fn load_change_impact_config(work_dir: &Path) -> Result<Option<ChangeImpactConfig>> {
    Ok(load_parsed_plan(work_dir)?.and_then(|plan| plan.metadata.loom.change_impact))
}
