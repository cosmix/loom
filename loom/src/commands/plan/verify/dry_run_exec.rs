//! The executor `loom plan verify --dry-run` runs the plan's commands with:
//! the same runner, confinement and verdict rules the daemon applies at
//! completion, pointed at the dry-run checkout.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result};

use super::dry_run::{run_dry_run_with, skip, stages_of, DryCommand};
use super::report::{VerifyError, VerifyReport};
use crate::models::stage::{CommandConfinement, Stage};
use crate::plan::parser::parse_plan_metadata;
use crate::plan::schema::StageDefinition;
use crate::process::sandbox_probe::confined_commands_blocker;
use crate::verify::criteria::assertions::{evaluate, AssertionSpec};
use crate::verify::criteria::{
    resolve_confinement, run_acceptance_with_config, run_spec_in_optional_tree, AcceptanceResult,
    CommandSpec, CriteriaConfig, CriterionResult,
};

/// The refusal for a shell where confined commands cannot start, naming the
/// probe's `why` and the remedy that fits it.
pub(super) fn needs_confinement(why: &str) -> String {
    let remedy = if why.contains("not found") {
        "install bubblewrap (bwrap)"
    } else {
        "run it from the operator's shell, where bubblewrap can create its namespaces"
    };
    format!(
        "loom plan verify --dry-run needs a shell where loom can confine commands ({why}); {remedy}"
    )
}

type Ran = Result<(CriterionResult, Vec<String>)>;
type StageTable<'a> = HashMap<&'a str, (&'a StageDefinition, &'a Stage)>;

/// Run the plan at `path` in a checkout of HEAD, every command under its confinement.
pub(super) fn run(path: &Path, report: &mut VerifyReport) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let Ok(metadata) = parse_plan_metadata(&text) else {
        return skip(report, "the plan metadata does not parse");
    };
    let loom = &metadata.loom;
    let plan_confinement = loom.sandbox.command_confinement;
    let local_binding = loom.sandbox.network.allow_local_binding;
    let stages = stages_of(path, loom);
    let confined = !loom.provision.is_empty()
        || stages.iter().any(|stage| {
            let level =
                resolve_confinement(stage.sandbox.command_confinement, Some(plan_confinement));
            level == CommandConfinement::Confined
        });
    if let Some(why) = confined.then(confined_commands_blocker).flatten() {
        report.errors.push(VerifyError {
            stage_id: None,
            message: needs_confinement(why),
        });
        report.valid = false;
        return Ok(());
    }
    let table: StageTable<'_> = loom
        .stages
        .iter()
        .zip(&stages)
        .map(|(definition, stage)| (definition.id.as_str(), (definition, stage)))
        .collect();
    run_dry_run_with(path, report, &mut |command: &DryCommand| {
        execute(command, &table, plan_confinement, local_binding)
    })
}

/// Run `command` as the daemon would. `Err` is a command the runner could not
/// prepare, start or wait for (a `working_dir` the stage itself creates), which
/// the dry run reports as not run.
fn execute(
    command: &DryCommand,
    stages: &StageTable<'_>,
    plan_confinement: CommandConfinement,
    local_binding: bool,
) -> Ran {
    match stages.get(command.stage_id.as_str()) {
        Some((_, stage)) if command.field == "acceptance" => {
            acceptance(command, stage, plan_confinement, local_binding)
        }
        Some((definition, _)) => assertion_check(command, definition),
        None => Err(anyhow::anyhow!(
            "stage `{}` is not in the plan",
            command.stage_id
        )),
    }
}

/// One acceptance criterion through the acceptance runner, cache off.
fn acceptance(
    command: &DryCommand,
    stage: &Stage,
    plan_confinement: CommandConfinement,
    local_binding: bool,
) -> Ran {
    let config = CriteriaConfig::default()
        .with_tree_root(Some(&command.tree_root))
        .with_only(Some(BTreeSet::from([command.index])))
        .with_plan_confinement(Some(plan_confinement))
        .with_plan_local_binding(local_binding);
    let (results, failures) = match run_acceptance_with_config(stage, Some(&command.dir), &config)?
    {
        AcceptanceResult::AllPassed { results } => (results, Vec::new()),
        AcceptanceResult::Failed { results, failures } => (results, failures),
    };
    let result = results
        .into_iter()
        .next()
        .context("the acceptance runner returned no result")?;
    Ok((result, failures))
}

/// An `after_stage` check or a wiring test, judged by completion's assertion
/// evaluation: `success` is the verdict, not the exit code.
fn assertion_check(command: &DryCommand, definition: &StageDefinition) -> Ran {
    let spec = CommandSpec::shell(command.command.as_str());
    let tree = Some(command.tree_root.as_path());
    let mut result = run_spec_in_optional_tree(
        &spec,
        tree,
        Some(&command.dir),
        command.timeout,
        command.confinement,
    )?;
    let failures = if command.field == "after_stage" {
        let check = definition
            .after_stage
            .get(command.index)
            .context("the after_stage check is not in the plan")?;
        let assertions = AssertionSpec {
            expected_exit: check.exit_code.unwrap_or(0),
            stdout_contains: &check.stdout_contains,
            stdout_not_contains: &check.stdout_not_contains,
            stderr_empty: check.stderr_empty == Some(true),
            ..Default::default()
        };
        evaluate(&assertions, &result).failures
    } else {
        let test = definition
            .wiring_tests
            .get(command.index)
            .context("the wiring test is not in the plan")?;
        let criteria = &test.success_criteria;
        let assertions = AssertionSpec {
            expected_exit: criteria.exit_code.unwrap_or(0),
            stdout_contains: &criteria.stdout_contains,
            stdout_not_contains: &criteria.stdout_not_contains,
            stderr_contains: &criteria.stderr_contains,
            stderr_empty: criteria.stderr_empty == Some(true),
        };
        evaluate(&assertions, &result).failures
    };
    result.success = failures.is_empty();
    Ok((result, failures))
}
