//! Verification-only checks used by the stage completion command: by the
//! operator's `loom stage complete` on the host, and by the daemon's
//! completion job (`host_verify::child_complete`).

use crate::models::stage::{Stage, StageStatus, StageType, TruthCheck};
use crate::plan::parser::{load_stage_definition_from_plan, parse_plan, ParsedPlan};
use crate::plan::schema::{CommandConfinement, StageDefinition};
use crate::verify::criteria::{plan_confinement, resolve_confinement};
use crate::verify::duplicate_detection::detect_duplicate_symbols;
use crate::verify::goal_backward::VerificationGap;
use crate::verify::wiring_detection::{detect_unwired_files, UnwiredFile};
use anyhow::{bail, Context, Result};
use std::path::Path;

#[path = "complete_verification_v2.rs"]
mod complete_verification_v2;

pub(crate) struct VerificationChecks<'a> {
    pub stage: &'a Stage,
    pub stage_id: &'a str,
    pub acceptance_dir: Option<&'a Path>,
    pub worktree_root: Option<&'a Path>,
    /// Leave test integrity and the review gate to the daemon, which checks
    /// them when it applies a completion job's result.
    pub daemon_gates: bool,
    /// Fail the completion when unwired-file or duplicate-symbol detection
    /// cannot run, instead of warning.
    pub fail_closed: bool,
    pub work_dir: &'a Path,
}

/// Where [`run_checks`] hands each check's label and outcome. Its `Err` stops
/// the run; an `Ok` for a failed check only records it.
pub(crate) type CheckSink<'s> = dyn FnMut(&'static str, Result<()>) -> Result<()> + 's;

/// The label of the impact-selected tests, whose failure a dispute names.
pub(crate) const IMPACT_TESTS_LABEL: &str = "impact-tests";

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

/// Every check, stopping at the first that fails: the operator's completion.
pub(super) fn run(checks: &VerificationChecks<'_>) -> Result<()> {
    run_checks(checks, &mut |_, checked| checked)
}

/// Run every check that applies to the stage, handing each one's label and
/// outcome to `sink`, which decides whether a failure stops the run.
pub(crate) fn run_checks(checks: &VerificationChecks<'_>, sink: &mut CheckSink<'_>) -> Result<()> {
    let repo_root = crate::fs::work_dir::WorkDir::new(checks.work_dir)?
        .main_project_root()
        .context("completion repository is unavailable")?;
    let base_branch = crate::fs::resolve_target_branch_from_config(checks.work_dir, &repo_root)?;
    let stage_def = load_stage_definition_from_plan(checks.stage_id, checks.work_dir)?;

    if stage_def
        .as_ref()
        .is_some_and(StageDefinition::has_any_goal_checks)
    {
        sink("goal-backward", run_goal_checks(checks))?;
    }
    if checks.stage.plan_version == 2 {
        complete_verification_v2::run_v2(checks, &base_branch, sink)?;
    }
    let after_stage = stage_def
        .as_ref()
        .map_or(&[][..], |def| &def.after_stage[..]);
    if !after_stage.is_empty() {
        sink("after-stage", run_after_checks(checks, after_stage))?;
    }
    let Some(worktree_root) = checks.worktree_root else {
        return Ok(());
    };
    sink(
        "unwired",
        run_unwired_check(checks, worktree_root, &base_branch),
    )?;
    sink(
        "duplicates",
        run_duplicate_check(checks, worktree_root, &base_branch),
    )?;
    if checks.stage.stage_type == StageType::IntegrationVerify {
        println!("Running aggregated wiring re-verification...");
        let checked = aggregated_wiring(worktree_root, checks.work_dir, checks.stage.plan_version);
        sink("aggregated-wiring", checked)?;
    }
    Ok(())
}

fn run_goal_checks(checks: &VerificationChecks<'_>) -> Result<()> {
    println!("Running goal-backward verification...");
    let verification_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
    let result = crate::commands::verify::run_and_verify_stage_goal(
        checks.stage_id,
        verification_dir,
        checks.worktree_root,
        checks.work_dir,
        checks.stage.plan_version,
    )?;
    if result.is_passed() {
        println!("Goal-backward verification passed!");
        return Ok(());
    }
    for gap in result.gaps() {
        eprintln!("  ✗ {:?}: {}", gap.gap_type, gap.description);
        eprintln!("    → {}", gap.suggestion);
    }
    crate::commands::stage::acceptance_runner::print_acceptance_failure_guidance(checks.stage_id);
    bail!(
        "Goal-backward verification failed for stage '{}'{}",
        checks.stage_id,
        gap_list(result.gaps())
    )
}

/// `: <gap>; <gap>` naming each gap, so a failure read from its message
/// alone (a host job's outcome) still says what is missing.
fn gap_list(gaps: &[VerificationGap]) -> String {
    if gaps.is_empty() {
        return String::new();
    }
    let descriptions: Vec<&str> = gaps.iter().map(|gap| gap.description.as_str()).collect();
    format!(": {}", descriptions.join("; "))
}

fn run_after_checks(checks: &VerificationChecks<'_>, after_stage: &[TruthCheck]) -> Result<()> {
    println!("Running after-stage verification...");
    let verification_dir = checks.acceptance_dir.unwrap_or(Path::new("."));
    let gaps = crate::verify::before_after::run_after_stage_checks(
        after_stage,
        verification_dir,
        checks.worktree_root,
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
        "After-stage verification failed for stage '{}'{}",
        checks.stage_id,
        gap_list(&gaps)
    )
}

fn run_unwired_check(
    checks: &VerificationChecks<'_>,
    worktree_root: &Path,
    base_branch: &str,
) -> Result<()> {
    let result = match detect_unwired_files(worktree_root, base_branch) {
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

fn run_duplicate_check(
    checks: &VerificationChecks<'_>,
    worktree_root: &Path,
    base_branch: &str,
) -> Result<()> {
    let duplicates = match detect_duplicate_symbols(worktree_root, base_branch) {
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

/// Re-verify the wiring of every completed stage. Every stage of one plan
/// shares `plan_version`, so the finishing stage's version applies to all.
fn aggregated_wiring(worktree_root: &Path, work_dir: &Path, plan_version: u32) -> Result<()> {
    let mut all_gaps = Vec::new();
    for definition in completed_definitions(work_dir)? {
        all_gaps.extend(stage_wiring_gaps(&definition, worktree_root, plan_version)?);
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

/// The plan definition of every `Completed` stage, in stage-list order.
fn completed_definitions(work_dir: &Path) -> Result<Vec<StageDefinition>> {
    let stages = crate::verify::transitions::list_all_stages(work_dir)?;
    let Some(plan) = load_parsed_plan(work_dir)? else {
        bail!("Could not load plan for aggregated re-verification");
    };
    let definitions = &plan.metadata.loom.stages;
    Ok(stages
        .iter()
        .filter(|stage| stage.status == StageStatus::Completed)
        .filter_map(|stage| definitions.iter().find(|item| item.id == stage.id).cloned())
        .collect())
}

fn stage_wiring_gaps(
    definition: &StageDefinition,
    worktree_root: &Path,
    plan_version: u32,
) -> Result<Vec<VerificationGap>> {
    if definition.wiring.is_empty() {
        return Ok(Vec::new());
    }
    let stage_id = &definition.id;
    println!("  Re-verifying wiring from stage '{stage_id}'...");
    let working_dir = if definition.working_dir == "." {
        worktree_root.to_path_buf()
    } else {
        worktree_root.join(&definition.working_dir)
    };
    let gaps = crate::verify::goal_backward::verify_wiring(
        &definition.wiring,
        &working_dir,
        plan_version,
    )?;
    for gap in &gaps {
        eprintln!("    ✗ {stage_id}: {}", gap.description);
    }
    Ok(gaps)
}

fn fail_closed_or_warn(
    checks: &VerificationChecks<'_>,
    label: &str,
    error: anyhow::Error,
) -> Result<()> {
    if checks.fail_closed {
        bail!("{label} could not run, so the completion fails: {error}");
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
