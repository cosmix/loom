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
use crate::verify::contracts::{
    contract_command, is_contract_or_harness, refusal, resolve_adapter,
};
use crate::verify::criteria::{
    plan_confinement, resolve_confinement, run_spec_with_timeout, CommandSpec,
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

    run_contracts(&site)
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
fn run_contracts(site: &ContractSite) -> Result<Vec<ContractRunReport>> {
    let stage = &site.stage;
    let confinement = resolve_confinement(
        stage.sandbox.command_confinement,
        plan_confinement(&site.work_dir),
    );
    let (mut reports, mut problems) = (Vec::new(), Vec::new());
    for contract in &stage.contracts {
        match run_contract(site, contract, confinement)? {
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

/// Run one contract and judge the run. The outer `Err` means the command
/// could not be run at all; the inner one is a problem the agent must fix.
fn run_contract(
    site: &ContractSite,
    contract: &ContractSpec,
    confinement: CommandConfinement,
) -> Result<Result<ContractRunReport, String>> {
    let adapter = resolve_adapter(contract, &site.working_dir);
    let command = contract_command(contract, adapter, &site.working_dir);
    println!("Running contract `{}`: {command}", contract.id);
    let run = run_spec_with_timeout(
        &CommandSpec::shell(command),
        Some(&site.working_dir),
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
mod tests {
    use super::*;
    use crate::testrun::registry;

    fn spec() -> ContractSpec {
        ContractSpec {
            id: "no-follow".to_string(),
            file: "tests/no_follow.rs".to_string(),
            test: "no_follow::rejects_symlink".to_string(),
            runner: Some("cargo-test".to_string()),
            scenario: "a symlink planted in the path".to_string(),
            rejects: "an open that follows symlinks".to_string(),
        }
    }

    fn cargo_test() -> Option<&'static dyn TestRunnerAdapter> {
        registry::by_name("cargo-test")
    }

    fn ran(executed: u64, failed: u64) -> RunSummary {
        RunSummary {
            executed: Some(executed),
            passed: Some(executed - failed),
            failed: Some(failed),
            ..RunSummary::default()
        }
    }

    #[test]
    fn freeze_rejects_non_contract_changes() {
        let contracts = [spec()];
        let harness = ["tests/support/**".to_string()];
        let changed = ["tests/no_follow.rs", "tests/support/fs.rs", "src/lib.rs"].map(String::from);
        let inputs = FreezeInputs {
            contracts: &contracts,
            harness: &harness,
            changed_paths: &changed,
            existing: &|_| true,
        };

        let problems = check_changes(&inputs).expect_err("src/lib.rs is not a contract file");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("src/lib.rs"), "{problems:?}");
    }

    #[test]
    fn freeze_rejects_passing_contract() {
        assert_eq!(classify(&ran(1, 0), Some(0)), RunOutcome::Passed);
        let error = judge_run(&spec(), cargo_test(), ran(1, 0), Some(0)).expect_err("green");
        assert!(error.contains("passes before implementation"), "{error}");
    }

    #[test]
    fn freeze_rejects_unselected_contract() {
        let error = judge_run(&spec(), cargo_test(), ran(0, 0), Some(0)).expect_err("unselected");
        assert!(error.contains("did not select"), "{error}");
    }

    #[test]
    fn freeze_reports_failing_and_unverified_runs() {
        let report = judge_run(&spec(), cargo_test(), ran(1, 1), Some(101)).expect("red run");
        assert_eq!(report.outcome, "failed");
        assert_eq!(report.adapter.as_deref(), Some("cargo-test"));
        assert_eq!(report.exit_code, Some(101));

        let report = judge_run(&spec(), None, RunSummary::default(), Some(2)).expect("exit 2");
        assert_eq!(report.outcome, UNVERIFIED);
        assert!(judge_run(&spec(), None, RunSummary::default(), Some(0)).is_err());
    }

    /// What a failed attempt leaves for the operator: a refusal's own
    /// problems, not its instructions, else the whole error.
    #[test]
    fn a_failed_attempt_lists_its_problems_for_the_operator() {
        let problems = vec!["src/lib.rs is neither a contract file".to_string()];
        let error = refuse("s1", problems.clone(), "Revert it, then").unwrap_err();
        assert!(error.to_string().contains("contracts freeze s1` again"));
        assert_eq!(listed_problems(&error), problems);

        let error = anyhow::anyhow!("no merge base").context("cannot find the stage base");
        assert_eq!(
            listed_problems(&error),
            ["cannot find the stage base: no merge base"]
        );
        assert!(refuse("s1", Vec::new(), "unused").is_ok());
    }
}
