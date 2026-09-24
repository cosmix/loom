//! The contract check at stage completion (DESIGN D9).
//!
//! Every frozen file still has its frozen content, and every contract passes.
//! Contracts run through the acceptance-criteria runner with the same
//! `CommandSpec` a criterion carrying the command would get, so an identical
//! earlier pass is reused from the certified cache. The caller gates this on a
//! v2 `standard` stage with contracts.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::store::{self, FreezeRecord};
use super::{contract_command, resolve_adapter};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::models::stage::Stage;
use crate::plan::schema::ContractSpec;
use crate::relay::sha256_hex;
use crate::testrun::{classify, RunOutcome, RunOutput, TestRunnerAdapter};
use crate::verify::criteria::{
    plan_confinement, CriteriaConfig, CriteriaProbe, ProbeRun, ProbeRunner,
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
            .with_cache_dir(work_dir),
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
    for contract in &stage.contracts {
        let adapter = resolve_adapter(contract, acceptance_dir);
        let command = contract_command(contract, adapter, acceptance_dir);
        let run = runner
            .run(&command, acceptance_dir)
            .with_context(|| format!("failed to run contract '{}'", contract.id))?;
        failures.extend(judge(contract, adapter, &command, &run));
    }
    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "contract check failed for stage '{id}':\n  - {}\n\
         Frozen contract files must keep their frozen content: restore them with \
         `loom stage contracts restore {id}`. A failing contract needs an implementation \
         that passes it.",
        failures.join("\n  - "),
        id = stage.id
    )
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
