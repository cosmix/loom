//! `loom stage contracts freeze`: prove every contract test fails before any
//! implementation exists, then ask the daemon to freeze the contract files
//! (DESIGN D8). The rules are two pure functions, [`check_changes`] and
//! [`judge_run`]; [`freeze`] gathers their real inputs. In a relayed session
//! it also keeps each attempt's outcome for the Stop hook
//! ([`crate::verify::contracts::refusal`]).

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::time::Duration;

use crate::daemon::ContractRunReport;
use crate::git::worktree::WorktreeGit;
use crate::models::stage::CommandConfinement;
use crate::plan::schema::ContractSpec;
use crate::relay::emit::{mode, EnvSnapshot, RelayMode, StdSink};
use crate::relay::RequestKind;
use crate::testrun::{classify, RunOutcome, RunOutput, RunSummary, TestRunnerAdapter};
use crate::verify::contracts::changes::{changed_paths, stage_base};
use crate::verify::contracts::format_gate::format_problems;
use crate::verify::contracts::run_dir::resolve_run;
use crate::verify::contracts::{is_contract_or_harness, refusal, resolve_adapter};
use crate::verify::criteria::{
    plan_confinement, resolve_confinement, run_spec_in_optional_tree, CommandSpec,
};

use super::{send_freeze, ContractSite};

/// How long one contract test may run.
const CONTRACT_TIMEOUT: Duration = Duration::from_secs(300);

/// Outcome of a run whose output said nothing readable but whose exit was not 0.
const UNVERIFIED: &str = "exit_nonzero_unverified";

/// What the contract session changed and which of its files exist; every
/// path is relative to the stage's `working_dir`.
pub(crate) struct FreezeInputs<'a> {
    pub contracts: &'a [ContractSpec],
    pub harness: &'a [String],
    pub changed_paths: &'a [String],
    pub existing: &'a dyn Fn(&str) -> bool,
}

/// D8 steps 1 and 2: every changed path is a contract file or matches a
/// `harness` glob, and every contract file exists.
pub(crate) fn check_changes(inputs: &FreezeInputs<'_>) -> Result<(), Vec<String>> {
    let mut problems: Vec<String> = inputs
        .changed_paths
        .iter()
        .filter(|path| !is_contract_or_harness(path.as_str(), inputs.contracts, inputs.harness))
        .map(|path| format!("{path} is neither a contract file nor matched by a `harness` glob"))
        .collect();
    problems.extend(
        inputs
            .contracts
            .iter()
            .filter(|contract| !(inputs.existing)(&contract.file))
            .map(|contract| {
                format!(
                    "contract `{}`: {} does not exist",
                    contract.id, contract.file
                )
            }),
    );
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// D8 step 3 for one run: `Failed` or `BuildFailed` is required. An adapter
/// that could not read the output, or no adapter at all, leaves only the
/// exit code, which must then be non-zero.
pub(crate) fn judge_run(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    summary: RunSummary,
    exit: Option<i32>,
) -> Result<ContractRunReport, String> {
    let id = &contract.id;
    let outcome = match adapter.map(|_| classify(&summary, exit)) {
        Some(RunOutcome::Failed) => "failed",
        Some(RunOutcome::BuildFailed) => "build_failed",
        Some(RunOutcome::Passed) => {
            return Err(format!(
                "contract `{id}` passes before implementation, so it cannot tell right from \
                 wrong: make {} assert behaviour the stage has not built yet",
                contract.file
            ))
        }
        Some(RunOutcome::NotSelected) => {
            return Err(format!(
                "contract `{id}`: the runner did not select the test `{}` in {}; its name \
                 must be exactly the one the runner filters by",
                contract.test, contract.file
            ))
        }
        Some(RunOutcome::Unparsed) | None if exit != Some(0) => UNVERIFIED,
        Some(RunOutcome::Unparsed) | None => {
            return Err(format!(
                "contract `{id}` exited 0 and its output showed no failing test, so nothing \
                 shows it fails"
            ))
        }
    };
    Ok(ContractRunReport {
        contract_id: id.clone(),
        adapter: adapter.map(|adapter| adapter.name().to_string()),
        outcome: outcome.to_string(),
        exit_code: exit,
    })
}

/// `loom stage contracts freeze <stage-id>`.
pub fn freeze(stage_id: String) -> Result<()> {
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let scratch_dir = match &relay_mode {
        RelayMode::Relay(context) => {
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            // SAFETY: `getuid` has no preconditions and cannot fail.
            let uid = unsafe { libc::getuid() };
            context.check(
                RequestKind::FreezeContracts,
                Some(stage_id.as_str()),
                &cwd,
                uid,
            )?;
            Some(context.scratch_dir.clone())
        }
        RelayMode::Legacy | RelayMode::Operator => None,
    };
    let outcome = checked_reports(&stage_id)
        .and_then(|reports| send_freeze(&stage_id, reports, relay_mode, &mut StdSink::default()));
    if let Some(scratch_dir) = scratch_dir {
        keep_outcome(&scratch_dir, &outcome);
    }
    outcome
}

/// D8 steps 1 to 3, run where the contract session runs: the changes, then
/// the red run, which yields the reports the daemon is sent. Git runs as the
/// worktree's `.git` directs it: this is the session's own process, and the
/// daemon checks the changes again with git pinned to the stage's registered
/// git directory before it freezes anything.
fn checked_reports(stage_id: &str) -> Result<Vec<ContractRunReport>> {
    let site = ContractSite::load(stage_id)?;
    let stage = &site.stage;
    if stage.contracts.is_empty() {
        bail!("Stage '{stage_id}' declares no contracts, so there is nothing to freeze");
    }

    let repo = WorktreeGit::discovered(&site.worktree_root);
    let base = stage_base(&repo, &site.work_dir)?;
    let changed = changed_paths(&repo, &site.working_dir, &base)?;
    let existing = |file: &str| site.working_dir.join(file).is_file();
    let inputs = FreezeInputs {
        contracts: &stage.contracts,
        harness: &stage.harness,
        changed_paths: &changed,
        existing: &existing,
    };
    refuse(
        stage_id,
        check_changes(&inputs).err().unwrap_or_default(),
        "The contract phase may change only contract files and files matching the stage's \
         `harness` globs. Revert every other change (`git checkout -- <path>`, or delete the \
         untracked file), then",
    )?;

    let confinement = resolve_confinement(
        stage.sandbox.command_confinement,
        plan_confinement(&site.work_dir),
    );
    check_formatting(&site, confinement)?;

    run_contracts(&site, confinement)
}

/// The formatter gate: a frozen file cannot change, so a contract or harness file
/// that fails one of the stage's formatter checks is refused before the freeze.
fn check_formatting(site: &ContractSite, confinement: CommandConfinement) -> Result<()> {
    let stage = &site.stage;
    let unformatted = format_problems(
        &stage.acceptance,
        &stage.setup,
        &site.worktree_root,
        &site.working_dir,
        confinement,
    )?;
    refuse(
        &stage.id,
        unformatted,
        "Every contract file and harness file must pass the stage's formatter checks before it is frozen: run the repository's formatter over them (for Rust, `cargo fmt --all`), then",
    )
}

/// Leave this attempt's outcome where the Stop hook looks once the writer
/// stops: a failure stays on record until an attempt reaches the daemon.
/// Best-effort; the attempt's own result is what the caller reports.
fn keep_outcome(scratch_dir: &Path, outcome: &Result<()>) {
    let kept = match outcome {
        Ok(()) => refusal::clear(scratch_dir),
        Err(error) => refusal::record(scratch_dir, &listed_problems(error)),
    };
    if let Err(error) = kept {
        eprintln!("warning: the operator cannot be shown this freeze's outcome: {error:#}");
    }
}

/// A freeze refused for the problems it lists, with the way out.
#[derive(Debug)]
struct Refusal {
    problems: Vec<String>,
    message: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

/// What a failed attempt tells the operator: the problems a refusal listed,
/// else the error the attempt failed with.
fn listed_problems(error: &anyhow::Error) -> Vec<String> {
    match error.downcast_ref::<Refusal>() {
        Some(refused) => refused.problems.clone(),
        None => vec![format!("{error:#}")],
    }
}

/// Fail with every problem listed, and the way out.
fn refuse(stage_id: &str, problems: Vec<String>, fix: &str) -> Result<()> {
    if problems.is_empty() {
        return Ok(());
    }
    let list: Vec<String> = problems
        .iter()
        .map(|problem| format!("  - {problem}"))
        .collect();
    let message = format!(
        "Contracts not frozen:\n{}\n{fix} run `loom stage contracts freeze {stage_id}` again.",
        list.join("\n")
    );
    Err(Refusal { problems, message }.into())
}

/// Run every contract with the criteria executor and judge each run.
fn run_contracts(
    site: &ContractSite,
    confinement: CommandConfinement,
) -> Result<Vec<ContractRunReport>> {
    let stage = &site.stage;
    let (mut reports, mut problems) = (Vec::new(), Vec::new());
    for contract in &stage.contracts {
        match run_contract(
            &site.worktree_root,
            &site.working_dir,
            contract,
            confinement,
        )? {
            Ok(report) => reports.push(report),
            Err(problem) => problems.push(problem),
        }
    }
    refuse(
        &stage.id,
        problems,
        "Every contract test must fail now, before any implementation exists; a failing \
         assertion and a compile or collection failure both count. Fix the tests named above, \
         then",
    )?;
    Ok(reports)
}

/// Run one contract from its run directory and judge the run. The outer
/// `Err` means the command could not be run at all; the inner one is a
/// problem the agent must fix.
fn run_contract(
    worktree_root: &Path,
    working_dir: &Path,
    contract: &ContractSpec,
    confinement: CommandConfinement,
) -> Result<Result<ContractRunReport, String>> {
    let adapter = resolve_adapter(contract, working_dir);
    let resolved = resolve_run(contract, adapter, worktree_root, working_dir, None)?;
    println!(
        "Running contract `{}` in {}: {}",
        contract.id, resolved.cwd, resolved.command
    );
    let run = run_spec_in_optional_tree(
        &CommandSpec::shell(resolved.command),
        Some(worktree_root),
        Some(&resolved.dir),
        CONTRACT_TIMEOUT,
        confinement,
    )?;
    if run.timed_out {
        return Ok(Err(format!(
            "contract `{}` did not finish within {} s",
            contract.id,
            CONTRACT_TIMEOUT.as_secs()
        )));
    }
    let output = RunOutput {
        stdout: &run.stdout,
        stderr: &run.stderr,
        exit_code: run.exit_code,
    };
    let summary = adapter.map_or_else(RunSummary::default, |adapter| adapter.parse(&output));
    let judged = judge_run(contract, adapter, summary, run.exit_code);
    if judged
        .as_ref()
        .is_ok_and(|report| report.outcome == UNVERIFIED)
    {
        let runner = adapter.map_or("no test-runner adapter", |adapter| adapter.name());
        eprintln!(
            "warning: contract `{}`: {runner} could not read its output; accepted on its \
             non-zero exit alone",
            contract.id
        );
    }
    Ok(judged)
}

#[cfg(test)]
#[path = "freeze_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "freeze_run_dir_tests.rs"]
mod run_dir_tests;
