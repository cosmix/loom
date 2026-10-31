//! A contract resolved for running: its adapter, run directory and command,
//! and the run itself.

use anyhow::Result;
use std::path::Path;

use super::{judge_run, CONTRACT_TIMEOUT, UNVERIFIED};
use crate::daemon::ContractRunReport;
use crate::models::stage::CommandConfinement;
use crate::plan::schema::ContractSpec;
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter};
use crate::verify::contracts::failed_builds::BuildKey;
use crate::verify::contracts::resolve_adapter;
use crate::verify::contracts::run_dir::{resolve_run, ContractRun};
use crate::verify::criteria::{run_spec_in_optional_tree, CommandSpec};

/// A contract resolved to its adapter and run directory, ready to run.
pub(crate) struct PreparedContract {
    adapter: Option<&'static dyn TestRunnerAdapter>,
    resolved: ContractRun,
}

impl PreparedContract {
    /// What two contracts share when running one repeats the other's build;
    /// `None` when no adapter applies, so nothing is known about what the
    /// command builds.
    pub(crate) fn build_key(&self) -> Option<BuildKey> {
        self.resolved.build_key()
    }
}

/// Resolve where and how `contract` runs.
pub(crate) fn prepare_contract(
    worktree_root: &Path,
    working_dir: &Path,
    contract: &ContractSpec,
) -> Result<PreparedContract> {
    let adapter = resolve_adapter(contract, working_dir);
    let resolved = resolve_run(contract, adapter, worktree_root, working_dir, None)?;
    Ok(PreparedContract { adapter, resolved })
}

/// A contract's judged run and whether the adapter reads its output as the
/// compiler rejecting the code (`TestRunnerAdapter::compile_rejected`).
pub(crate) struct JudgedRun {
    pub judged: Result<ContractRunReport, String>,
    pub compile_failed: bool,
}

/// Run one contract resolved by [`prepare_contract`] from its run directory with
/// the criteria executor and judge the run. The outer `Err` means the command
/// could not be run at all; the inner one is a problem the agent must fix.
pub(crate) fn run_prepared(
    worktree_root: &Path,
    contract: &ContractSpec,
    prepared: &PreparedContract,
    confinement: CommandConfinement,
    emit: fn(&str),
) -> Result<JudgedRun> {
    let adapter = prepared.adapter;
    let resolved = &prepared.resolved;
    emit(&format!(
        "Running contract `{}` in {}: {}",
        contract.id, resolved.cwd, resolved.command
    ));
    let run = run_spec_in_optional_tree(
        &CommandSpec::shell(resolved.command.clone()),
        Some(worktree_root),
        Some(&resolved.dir),
        CONTRACT_TIMEOUT,
        confinement,
    )?;
    if run.timed_out {
        return Ok(JudgedRun {
            judged: Err(format!(
                "contract `{}` did not finish within {} s",
                contract.id,
                CONTRACT_TIMEOUT.as_secs()
            )),
            compile_failed: false,
        });
    }
    let output = RunOutput {
        stdout: &run.stdout,
        stderr: &run.stderr,
        exit_code: run.exit_code,
    };
    let summary = adapter.map_or_else(RunSummary::default, |adapter| adapter.parse(&output));
    let judged = judge_run(contract, adapter, summary, run.exit_code);
    warn_if_unverified(contract, adapter, &judged, emit);
    let compile_failed = adapter.is_some_and(|adapter| adapter.compile_rejected(&output));
    Ok(JudgedRun {
        judged,
        compile_failed,
    })
}

/// Warn when the runner could not read the output and the contract was accepted
/// on its non-zero exit alone.
fn warn_if_unverified(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    judged: &Result<ContractRunReport, String>,
    emit: fn(&str),
) {
    if judged
        .as_ref()
        .is_ok_and(|report| report.outcome == UNVERIFIED)
    {
        let runner = adapter.map_or("no test-runner adapter", |adapter| adapter.name());
        emit(&format!(
            "warning: contract `{}`: {runner} could not read its output; accepted on its \
             non-zero exit alone",
            contract.id
        ));
    }
}
