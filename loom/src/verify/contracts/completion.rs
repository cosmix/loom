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
use super::run_dir::resolve_run;
use super::store::{self, FreezeRecord};
use super::{normalize, resolve_adapter};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::models::stage::Stage;
use crate::plan::schema::ContractSpec;
use crate::relay::sha256_hex;
use crate::testrun::{classify, registry, RunOutcome, RunOutput, TestRunnerAdapter};
use crate::verify::criteria::{
    plan_confinement, CachedRun, CriteriaConfig, CriteriaProbe, ProbeRun, ProbeRunner,
};

/// Fail unless every frozen file is unchanged and every contract passes.
pub fn check(
    stage: &Stage,
    work_dir: &Path,
    acceptance_dir: &Path,
    worktree_root: &Path,
) -> Result<()> {
    let runner = CriteriaProbe {
        stage,
        config: CriteriaConfig::default()
            .with_plan_confinement(plan_confinement(work_dir))
            .with_cache_dir(work_dir)
            .with_tree_root(Some(worktree_root)),
    };
    check_with(stage, work_dir, acceptance_dir, worktree_root, &runner)
}

fn check_with(
    stage: &Stage,
    work_dir: &Path,
    acceptance_dir: &Path,
    worktree_root: &Path,
    runner: &dyn ProbeRunner,
) -> Result<()> {
    let Some(record) = store::load_freeze(work_dir, &stage.id)? else {
        bail!(
            "the contracts of stage '{}' were never frozen: the contract phase did not finish",
            stage.id
        );
    };
    let mut failures = changed_frozen_files(&record, acceptance_dir, worktree_root)?;
    let place = Place {
        record: &record,
        acceptance_dir,
        worktree_root,
    };
    for contract in &stage.contracts {
        let recorded = store::recorded_cwd(&record, contract);
        let found = run_contract(contract, recorded, &place, runner)?;
        failures.extend(found);
    }
    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "contract check failed for stage '{id}':\n  - {}\n\
         Each frozen contract file must keep its frozen content: restore it with \
         `loom stage contracts restore {id}` or dispute it with \
         `loom stage dispute-contract {id} --contract <id> --reason ...`. A failing contract \
         needs an implementation that passes it. A contract module must be declared by a \
         plain `mod` item, and its chain files must not be cfg'd out.",
        failures.join("\n  - "),
        id = stage.id
    )
}

/// Where a contract runs: its stage's freeze record and directories.
struct Place<'a> {
    record: &'a FreezeRecord,
    acceptance_dir: &'a Path,
    worktree_root: &'a Path,
}

/// Run `contract` from its run directory (its plan `cwd`, else the one
/// `recorded` for its file by the freeze, else resolved as the freeze
/// resolves it) and say why it does not pass, if it
/// does not. A contract that cannot be resolved is a failure of its own, so
/// the check still runs and reports every other contract. A Rust contract's
/// collection (its module chain and run directory) is checked whether or not
/// the run is cached, since a cached pass would otherwise hide a redirect.
fn run_contract(
    contract: &ContractSpec,
    recorded: Option<&str>,
    place: &Place<'_>,
    runner: &dyn ProbeRunner,
) -> Result<Vec<String>> {
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
            return Ok(vec![format!(
                "contract '{}' cannot be run: {error:#}",
                contract.id
            )])
        }
    };
    let mut failures = collection_problems(contract, place, &resolved.cwd)?;
    let run = runner
        .run(&resolved.command, &resolved.dir)
        .with_context(|| format!("failed to run contract '{}'", contract.id))?;
    failures.extend(judge(contract, adapter, &resolved.command, &run));
    Ok(failures)
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
    let mut problems = chain_problems(&source, &file, &contract.test);
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
    let root = worktree_root.canonicalize()?;
    let working = acceptance_dir.canonicalize()?;
    let inside = working
        .strip_prefix(&root)
        .context("the acceptance directory is outside the worktree")?;
    let changed = record.files.iter().filter_map(|file| {
        let path = &file.path;
        match read_bounded(&root, &inside.join(path), store::MAX_FROZEN_FILE_BYTES) {
            Ok(bytes) if sha256_hex(&bytes) == file.sha256 => None,
            Ok(_) => Some(format!("frozen file '{path}' changed after the freeze")),
            Err(error) if is_not_found(&error) => Some(format!("frozen file '{path}' is missing")),
            Err(error) => Some(format!(
                "frozen file '{path}' cannot be read as frozen: {error:#}"
            )),
        }
    });
    Ok(changed.collect())
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
            contract.test
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
            contract.test
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
