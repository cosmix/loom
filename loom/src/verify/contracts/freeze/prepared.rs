//! A contract resolved for running: its adapter, run directory and command,
//! and the run itself.

use anyhow::Result;
use std::path::Path;

use super::{judge_run, BUILD_FAILED, CONTRACT_TIMEOUT, UNVERIFIED};
use crate::daemon::ContractRunReport;
use crate::models::dispute::tail_to_byte_limit;
use crate::models::stage::CommandConfinement;
use crate::plan::schema::ContractSpec;
use crate::stage_cache::StageCache;
use crate::testrun::recognize::combined_output;
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter};
use crate::verify::contracts::failed_builds::BuildKey;
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::contracts::resolve_adapter;
use crate::verify::contracts::run_dir::{resolve_run, ContractRun};
use crate::verify::criteria::{run_spec_in_optional_stage, CommandSpec, CriterionResult};

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

    /// Whether a test-runner adapter builds the command; without one the
    /// contract's test runs as a shell command.
    pub(crate) fn has_adapter(&self) -> bool {
        self.adapter.is_some()
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
/// A confined run also uses `cache`, the stage's build caches.
pub(crate) fn run_prepared(
    worktree_root: &Path,
    contract: &ContractSpec,
    prepared: &PreparedContract,
    confinement: CommandConfinement,
    cache: Option<&StageCache>,
    emit: fn(&str),
) -> Result<JudgedRun> {
    let adapter = prepared.adapter;
    let resolved = &prepared.resolved;
    emit(&format!(
        "Running contract `{}` in {}: {}",
        contract.id, resolved.cwd, resolved.command
    ));
    let run = run_spec_in_optional_stage(
        &CommandSpec::shell(resolved.command.clone()),
        Some(worktree_root),
        Some(&resolved.dir),
        CONTRACT_TIMEOUT,
        confinement,
        cache,
    )?;
    Ok(judge_prepared_result(contract, adapter, &run, emit))
}

fn judge_prepared_result(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    run: &CriterionResult,
    emit: fn(&str),
) -> JudgedRun {
    if run.timed_out {
        return JudgedRun {
            judged: Err(format!(
                "contract `{}` did not finish within {} s",
                contract.id,
                CONTRACT_TIMEOUT.as_secs()
            )),
            compile_failed: false,
        };
    }
    if !run.output_complete {
        return JudgedRun {
            judged: Err(format!(
                "contract `{}` output capture was incomplete",
                contract.id
            )),
            compile_failed: false,
        };
    }
    let output = RunOutput {
        stdout: &run.stdout,
        stderr: &run.stderr,
        exit_code: run.exit_code,
    };
    let summary = adapter.map_or_else(RunSummary::default, |adapter| adapter.parse(&output));
    let judged = judge_run(
        contract,
        adapter,
        summary,
        output.exit_code,
        Some((output.stdout, output.stderr)),
    );
    warn_if_unverified(contract, adapter, &judged, emit);
    let compile_failed = adapter.is_some_and(|adapter| adapter.compile_rejected(&output));
    if !compile_failed {
        warn_if_build_stopped(contract, &judged, &output, emit);
    }
    JudgedRun {
        judged,
        compile_failed,
    }
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

/// The most of a run's output the build-stopped warning quotes, from its end.
const STOPPED_BUILD_TAIL_BYTES: usize = 2048;

/// Warn, quoting the end of the output, when the runner read a build failure
/// that is not the compiler rejecting the code: the toolchain or the boundary
/// stopped the build, so contracts sharing it run their own build and nothing
/// else records why.
fn warn_if_build_stopped(
    contract: &ContractSpec,
    judged: &Result<ContractRunReport, String>,
    output: &RunOutput<'_>,
    emit: fn(&str),
) {
    if !judged
        .as_ref()
        .is_ok_and(|report| report.outcome == BUILD_FAILED)
    {
        return;
    }
    let tail = tail_to_byte_limit(&combined_output(output), STOPPED_BUILD_TAIL_BYTES);
    emit(&format!(
        "warning: contract `{}`: the build stopped before the compiler judged the code, so \
         no other contract shares it; its output ends:\n{}",
        contract.id,
        run_output_line(&tail)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::registry;
    use crate::verify::contracts::test_support::contract;
    use std::time::Duration;

    #[test]
    fn incomplete_capture_cannot_be_a_frozen_failure_or_shared_build() {
        let contract = contract();
        let adapter = registry::by_name("cargo-test");
        for (stdout, stderr, exit) in [
            ("test result: FAILED. 0 passed; 1 failed", "", Some(101)),
            ("", "error: could not compile package", Some(101)),
            ("test result: ok. 1 passed; 0 failed", "", Some(0)),
        ] {
            let mut run = CriterionResult::new(
                "cargo test".to_string(),
                false,
                stdout.to_string(),
                stderr.to_string(),
                exit,
                Duration::ZERO,
                false,
            );
            run.output_complete = false;
            let result = judge_prepared_result(&contract, adapter, &run, |_| {});
            assert!(result
                .judged
                .unwrap_err()
                .contains("output capture was incomplete"));
            assert!(!result.compile_failed);
        }
    }

    std::thread_local! {
        static EMITTED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn record(line: &str) {
        EMITTED.with(|lines| lines.borrow_mut().push(line.to_string()));
    }

    /// The emitted lines of judging one cargo run that exited 101 with `stderr`.
    fn judged_lines(stderr: &str) -> (JudgedRun, Vec<String>) {
        EMITTED.with(|lines| lines.borrow_mut().clear());
        let run = CriterionResult::new(
            "cargo test".to_string(),
            false,
            String::new(),
            stderr.to_string(),
            Some(101),
            Duration::ZERO,
            false,
        );
        let adapter = registry::by_name("cargo-test");
        let judged = judge_prepared_result(&contract(), adapter, &run, record);
        (judged, EMITTED.with(|lines| lines.borrow().clone()))
    }

    #[test]
    fn a_build_stopped_before_the_compiler_is_warned_with_its_output() {
        let (judged, lines) = judged_lines(
            "error: failed to run `rustc` to learn about target-specific information\n",
        );
        assert_eq!(judged.judged.unwrap().outcome, BUILD_FAILED);
        assert!(!judged.compile_failed);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("the build stopped"), "{lines:?}");
        assert!(
            lines[0].contains("failed to run `rustc` to learn about target-specific"),
            "{lines:?}"
        );
        assert_eq!(lines[0].lines().count(), 2, "the output is one quoted line");
    }

    #[test]
    fn a_compile_rejection_is_not_warned() {
        let (judged, lines) =
            judged_lines("error: could not compile `fx` (test \"x\") due to 1 previous error\n");
        assert!(judged.compile_failed);
        assert!(lines.is_empty(), "{lines:?}");
    }
}
