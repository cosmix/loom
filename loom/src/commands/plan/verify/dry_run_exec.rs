//! The executor `loom plan verify --dry-run` runs the plan's commands with:
//! the same runner, confinement and verdict rules the daemon applies at
//! completion, pointed at the dry-run checkout.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};

use super::dry_run::{run_dry_run_with, skip, stages_of, DryCommand};
use super::report::{parse_metadata, VerifyError, VerifyReport};
use crate::models::stage::{CommandConfinement, Stage};
use crate::plan::schema::StageDefinition;
use crate::process::sandbox_probe::confined_commands_runnable;
use crate::verify::criteria::assertions::{evaluate, AssertionSpec};
use crate::verify::criteria::{
    resolve_confinement, run_acceptance_with_config, run_spec_in_optional_tree, AcceptanceResult,
    CommandSpec, CriteriaConfig, CriterionResult,
};

const NEEDS_CONFINEMENT: &str = "loom plan verify --dry-run needs a shell where loom can \
     confine commands (bubblewrap cannot create its namespaces here); run it from the \
     operator's shell";

type Ran = Result<(CriterionResult, Vec<String>)>;
type StageTable<'a> = HashMap<&'a str, (&'a StageDefinition, &'a Stage)>;

/// Run the plan at `path` in a checkout of HEAD, every command under its confinement.
pub(super) fn run(path: &Path, report: &mut VerifyReport) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let Some(metadata) = parse_metadata(&text) else {
        return skip(report, "the plan metadata does not parse");
    };
    let loom = &metadata.loom;
    let plan_confinement = loom.sandbox.command_confinement;
    let stages = stages_of(path, loom);
    let confined = !loom.provision.is_empty()
        || stages.iter().any(|stage| {
            let level =
                resolve_confinement(stage.sandbox.command_confinement, Some(plan_confinement));
            level == CommandConfinement::Confined
        });
    if confined && !confined_commands_runnable() {
        report.errors.push(VerifyError {
            stage_id: None,
            message: NEEDS_CONFINEMENT.to_string(),
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
        execute(command, &table, plan_confinement)
    })
}

/// Run `command` as the daemon would. A command the runner could not prepare
/// (a `working_dir` the stage itself creates) is a failed result, not an error.
fn execute(
    command: &DryCommand,
    stages: &StageTable<'_>,
    plan_confinement: CommandConfinement,
) -> (CriterionResult, Vec<String>) {
    let ran = match stages.get(command.stage_id.as_str()) {
        Some((_, stage)) if command.field == "acceptance" => {
            acceptance(command, stage, plan_confinement)
        }
        Some((definition, _)) => assertion_check(command, definition),
        None => Err(anyhow::anyhow!(
            "stage `{}` is not in the plan",
            command.stage_id
        )),
    };
    ran.unwrap_or_else(|error| {
        let message = format!("{error:#}");
        let reason = message.lines().next().unwrap_or_default().to_string();
        let result = CriterionResult::new(
            command.command.clone(),
            false,
            String::new(),
            reason,
            None,
            Duration::ZERO,
            false,
        );
        (result, Vec::new())
    })
}

/// One acceptance criterion through the acceptance runner, cache off.
fn acceptance(command: &DryCommand, stage: &Stage, plan_confinement: CommandConfinement) -> Ran {
    let config = CriteriaConfig::default()
        .with_tree_root(Some(&command.tree_root))
        .with_only(Some(BTreeSet::from([command.index])))
        .with_plan_confinement(Some(plan_confinement));
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
