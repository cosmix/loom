//! The contract check at stage completion (DESIGN D9).
//!
//! Every frozen file still has its frozen content, and every contract passes,
//! run from the directory `run_dir` resolves for it. Contracts
//! run through the acceptance-criteria runner with the same
//! `CommandSpec` a criterion carrying the command would get, so an identical
//! earlier pass is reused from the certified cache. The caller gates this on a
//! v2 `standard` stage with contracts.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::collection::rust::{chain_problems, run_dir_problems};
use super::drift::{frozen_drift, Drift, DriftKind};
use super::failed_builds::FailedBuilds;
use super::run_dir::resolve_run;
use super::store::{self, FreezeRecord};
use super::{normalize, resolve_adapter};
use crate::fs::safe_read::read_bounded;
use crate::models::stage::{Stage, StageType};
use crate::plan::schema::ContractSpec;
use crate::testrun::{classify, registry, RunOutcome, RunOutput, TestRunnerAdapter};
#[path = "completion_report.rs"]
mod report;
pub use report::{guidance, ContractCheck, ContractVerdict};

use super::trial::TrialFile;
use crate::verify::criteria::environment::environment_marker;
use crate::verify::criteria::{
    confined_stage_cache, plan_confinement, resolve_confinement, CachePolicy, CachedRun,
    CriteriaConfig, CriteriaProbe, ProbeRun, ProbeRunner,
};

/// Fail unless every frozen file is unchanged and every contract passes.
pub fn check(
    stage: &Stage,
    work_dir: &Path,
    acceptance_dir: &Path,
    worktree_root: &Path,
) -> Result<()> {
    run(stage, work_dir, acceptance_dir, worktree_root, &[])?.into_result(&stage.id)
}

/// Whether a check of every criterion (`loom check <stage>`) also runs the
/// contracts: a `standard` stage with contracts that were frozen, so the
/// gate run before completion shows what the completion check would refuse.
pub fn runs_in_full_check(stage: &Stage, work_dir: &Path) -> Result<bool> {
    Ok(stage.stage_type == StageType::Standard
        && !stage.contracts.is_empty()
        && store::load_freeze(work_dir, &stage.id)?.is_some())
}

/// What the check finds, without failing on it. `overrides` replace the
/// sha256 the freeze recorded for those frozen files: a replacement trial
/// expects its replacement bytes there, and only the trial passes any. A
/// trial's runs neither read nor store the pass cache: the bytes they ran on
/// are gone once the trial restores the frozen files.
pub fn run(
    stage: &Stage,
    work_dir: &Path,
    acceptance_dir: &Path,
    worktree_root: &Path,
    overrides: &[TrialFile],
) -> Result<ContractCheck> {
    let plan = plan_confinement(work_dir);
    let confinement = resolve_confinement(stage.sandbox.command_confinement, plan);
    let runner = CriteriaProbe {
        stage,
        config: CriteriaConfig::default()
            .with_plan_confinement(plan)
            .with_cache_dir(work_dir)
            .with_tree_root(Some(worktree_root))
            .with_stage_cache(confined_stage_cache(work_dir, &stage.id, confinement)?),
    };
    let runner = if overrides.is_empty() {
        runner
    } else {
        CriteriaProbe {
            config: runner.config.with_cache_policy(CachePolicy::Bypass),
            ..runner
        }
    };
    let place = (acceptance_dir, worktree_root);
    findings_with(stage, work_dir, place, &runner, overrides)
}

/// [`run`] with `runner`; `place` is the acceptance directory and the
/// worktree root.
fn findings_with(
    stage: &Stage,
    work_dir: &Path,
    (acceptance_dir, worktree_root): (&Path, &Path),
    runner: &dyn ProbeRunner,
    overrides: &[TrialFile],
) -> Result<ContractCheck> {
    let Some(mut record) = store::load_freeze(work_dir, &stage.id)? else {
        bail!(
            "the contracts of stage '{}' were never frozen: the contract phase did not finish",
            stage.id
        );
    };
    for file in &mut record.files {
        if let Some(trial) = overrides.iter().find(|trial| trial.path == file.path) {
            file.sha256.clone_from(&trial.sha256);
        }
    }
    let drift = changed_frozen_files(&record, acceptance_dir, worktree_root)?;
    let place = Place {
        record: &record,
        acceptance_dir,
        worktree_root,
    };
    let mut rejected = FailedBuilds::default();
    let mut contracts = Vec::with_capacity(stage.contracts.len());
    for contract in &stage.contracts {
        // The contract as the freeze ran it: the recorded test and adapter
        // fill what the plan left open.
        let effective = store::effective_contract(&record, contract);
        let recorded = store::recorded_cwd(&record, contract);
        let (failures, marker) = run_contract(&effective, recorded, &place, runner, &mut rejected)?;
        let id = contract.id.clone();
        contracts.push(ContractVerdict {
            id,
            failures,
            marker,
        });
    }
    Ok(ContractCheck { drift, contracts })
}

#[cfg(test)]
fn check_with(
    stage: &Stage,
    work_dir: &Path,
    acceptance_dir: &Path,
    worktree_root: &Path,
    runner: &dyn ProbeRunner,
) -> Result<()> {
    let place = (acceptance_dir, worktree_root);
    findings_with(stage, work_dir, place, runner, &[])?.into_result(&stage.id)
}

/// Where a contract runs: its stage's freeze record and directories.
struct Place<'a> {
    record: &'a FreezeRecord,
    acceptance_dir: &'a Path,
    worktree_root: &'a Path,
}

/// Run `contract`, the effective spec (`store::effective_contract`: the
/// recorded test and adapter fill what the plan left open), from its run
/// directory (its plan `cwd`, else the one
/// `recorded` for its file by the freeze, else resolved as the freeze
/// resolves it) and say why it does not pass, if it does not, with the
/// environment marker its failing output printed. A contract that cannot be resolved is a failure of its own, so
/// the check still runs and reports every other contract. A Rust contract's
/// collection (its module chain and run directory) is checked whether or not
/// the run is cached, since a cached pass would otherwise hide a redirect.
/// A contract whose build an earlier contract of `rejected` already had the
/// compiler reject is not run: it fails naming that contract, and so records
/// nothing in the certified cache.
fn run_contract(
    contract: &ContractSpec,
    recorded: Option<&str>,
    place: &Place<'_>,
    runner: &dyn ProbeRunner,
    rejected: &mut FailedBuilds<()>,
) -> Result<(Vec<String>, Option<&'static str>)> {
    let adapter = resolve_adapter(contract, place.acceptance_dir);
    let resolved = match resolve_run(
        contract,
        adapter,
        place.worktree_root,
        place.acceptance_dir,
        recorded,
    ) {
        Ok(resolved) => resolved,
        Err(error) => {
            let failure = format!("contract '{}' cannot be run: {error:#}", contract.id);
            return Ok((vec![failure], None));
        }
    };
    let mut failures = collection_problems(contract, place, &resolved.cwd)?;
    let build = resolved.build_key();
    if let Some((first, ())) = build.as_ref().and_then(|build| rejected.get(build)) {
        failures.push(format!(
            "contract '{}' was not run: the build it shares with contract '{first}' failed",
            contract.id
        ));
        return Ok((failures, None));
    }
    let run = runner
        .run(&resolved.command, &resolved.dir)
        .with_context(|| format!("failed to run contract '{}'", contract.id))?;
    if let (Some(build), Some(adapter)) = (build, adapter) {
        if compile_rejected(adapter, &run) {
            rejected.record(build, contract.id.clone(), ());
        }
    }
    let Some(failure) = judge(contract, adapter, &resolved.command, &run) else {
        return Ok((failures, None));
    };
    failures.push(failure);
    Ok((failures, environment_marker(&run.stdout, &run.stderr)))
}

/// Whether `run` ended with the compiler rejecting the code. A cached run is a
/// certified pass, and a timed-out one has no output to read.
fn compile_rejected(adapter: &dyn TestRunnerAdapter, run: &ProbeRun) -> bool {
    !run.timed_out
        && run.output_complete
        && run.cached.is_none()
        && adapter.compile_rejected(&RunOutput {
            stdout: &run.stdout,
            stderr: &run.stderr,
            exit_code: run.exit_code,
        })
}

/// Whether `contract` runs through a Rust adapter: its `runner`, else the
/// adapter the freeze recorded (none recorded: not Rust), never one
/// re-detected from the implementer's tree. Only a contract the record does
/// not list is detected.
fn is_rust(contract: &ContractSpec, record: &FreezeRecord, acceptance_dir: &Path) -> bool {
    let entry = record
        .contracts
        .iter()
        .find(|entry| entry.id == contract.id);
    let adapter = match (contract.runner.as_deref(), entry) {
        (Some(name), _) => registry::by_name(name),
        (None, Some(entry)) => entry.adapter.as_deref().and_then(registry::by_name),
        (None, None) => resolve_adapter(contract, acceptance_dir),
    };
    adapter.is_some_and(|adapter| adapter.language() == "rust")
}

/// The chain and run-directory problems of a Rust contract, each as
/// `contract '<id>': <problem>`; `run_cwd` is its run directory relative to
/// the worktree root.
fn collection_problems(
    contract: &ContractSpec,
    place: &Place<'_>,
    run_cwd: &str,
) -> Result<Vec<String>> {
    if !is_rust(contract, place.record, place.acceptance_dir) {
        return Ok(Vec::new());
    }
    let root = place.worktree_root.canonicalize()?;
    let working = place.acceptance_dir.canonicalize()?;
    let inside = working
        .strip_prefix(&root)
        .context("the acceptance directory is outside the worktree")?;
    let source = |path: &str| {
        let bytes = read_bounded(&root, Path::new(path), store::MAX_FROZEN_FILE_BYTES).ok()?;
        String::from_utf8(bytes).ok()
    };
    let prefix = inside.to_string_lossy().replace('\\', "/");
    let file = normalize(&contract.file);
    let file = if prefix.is_empty() {
        file
    } else {
        format!("{prefix}/{file}")
    };
    // `resolve_run` has already required the test.
    let mut problems = chain_problems(&source, &file, contract.required_test()?);
    problems.extend(run_dir_problems(&source, &file, run_cwd));
    Ok(problems
        .into_iter()
        .map(|problem| format!("contract '{}': {problem}", contract.id))
        .collect())
}

/// Frozen files whose content differs from the freeze, or that are gone.
fn changed_frozen_files(
    record: &FreezeRecord,
    acceptance_dir: &Path,
    worktree_root: &Path,
) -> Result<Vec<String>> {
    let drift = frozen_drift(record, acceptance_dir, worktree_root)?;
    Ok(drift
        .into_iter()
        .map(|Drift { path, kind }| match kind {
            DriftKind::Changed => format!("frozen file '{path}' changed after the freeze"),
            DriftKind::Missing => format!("frozen file '{path}' is missing"),
            DriftKind::Unreadable(error) => {
                format!("frozen file '{path}' cannot be read as frozen: {error}")
            }
        })
        .collect())
}

/// Why `run` does not count as a pass, if it does not.
fn judge(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    command: &str,
    run: &ProbeRun,
) -> Option<String> {
    let id = &contract.id;
    if run.timed_out {
        return Some(format!("contract '{id}' timed out running `{command}`"));
    }
    if !run.output_complete {
        return Some(format!(
            "contract '{id}' output capture was incomplete: `{command}`"
        ));
    }
    if let Some(cached) = &run.cached {
        return judge_cached(contract, adapter, command, run.exit_code, cached);
    }
    let output = RunOutput {
        stdout: &run.stdout,
        stderr: &run.stderr,
        exit_code: run.exit_code,
    };
    let outcome = adapter.map(|adapter| classify(&adapter.parse(&output), run.exit_code));
    match outcome {
        Some(RunOutcome::Passed) => None,
        Some(RunOutcome::Failed) => Some(format!("contract '{id}' fails: `{command}`")),
        Some(RunOutcome::BuildFailed) => {
            Some(format!("contract '{id}' does not build: `{command}`"))
        }
        Some(RunOutcome::NotSelected) => Some(format!(
            "contract '{id}': contract test not selected; `{command}` ran no test named '{}'",
            contract.test.as_deref().unwrap_or_default()
        )),
        Some(RunOutcome::Unparsed) | None => judge_exit_code(id, adapter, command, run.exit_code),
    }
}

/// A certified cache hit already proved the exit code and, under the
/// zero-test guard, the count of tests run. Its output is a tail that may hold
/// only the trailing `running 0 tests` blocks of other test binaries, so the
/// recorded count decides instead of a re-parse.
fn judge_cached(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    command: &str,
    exit_code: Option<i32>,
    cached: &CachedRun,
) -> Option<String> {
    let id = &contract.id;
    match cached.tests_executed {
        Some(0) => Some(format!(
            "contract '{id}': contract test not selected; `{command}` ran no test named '{}'",
            contract.test.as_deref().unwrap_or_default()
        )),
        Some(_) if exit_code == Some(0) => None,
        _ => judge_exit_code(id, adapter, command, exit_code),
    }
}

/// No readable test counts: the exit code decides, and a pass says so.
fn judge_exit_code(
    id: &str,
    adapter: Option<&dyn TestRunnerAdapter>,
    command: &str,
    exit_code: Option<i32>,
) -> Option<String> {
    let runner = adapter.map_or("no test-runner adapter", |adapter| adapter.name());
    if exit_code == Some(0) {
        eprintln!(
            "warning: contract '{id}' passed on its exit code alone ({runner} output not parsed)"
        );
        return None;
    }
    let exit = exit_code.map_or_else(|| "a signal".to_string(), |code| format!("exit {code}"));
    Some(format!(
        "contract '{id}' ended with {exit} ({runner} output not parsed): `{command}`"
    ))
}

#[cfg(test)]
#[path = "completion_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "completion_adapter_tests.rs"]
mod adapter_tests;

#[cfg(test)]
#[path = "completion_share_tests.rs"]
mod share_tests;
