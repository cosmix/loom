//! The zero-test guard (DESIGN D10): on a v2 stage, a criterion whose test
//! runner selected no test fails even though the runner exited 0.

use std::path::Path;

use super::result::CriterionResult;
use crate::testrun::{registry, RunOutput, RunSummary, TestRunnerAdapter};

/// The tests a run of `command` in `cwd` executed, as the adapter recognising
/// `command` reads them from `result`; `None` when no adapter recognises it or
/// the output reports no count.
pub(super) fn tests_executed(command: &str, cwd: &Path, result: &CriterionResult) -> Option<u64> {
    recognized_summary(command, cwd, result).and_then(|(_, summary)| summary.executed)
}

/// `selected zero tests (<adapter>)` when the adapter recognising `command`
/// reads a run of zero tests from `result`.
pub(super) fn zero_test_failure(
    command: &str,
    cwd: &Path,
    result: &CriterionResult,
) -> Option<String> {
    let (adapter, summary) = recognized_summary(command, cwd, result)?;
    (summary.executed == Some(0)).then(|| format!("selected zero tests ({})", adapter.name()))
}

fn recognized_summary(
    command: &str,
    cwd: &Path,
    result: &CriterionResult,
) -> Option<(&'static dyn TestRunnerAdapter, RunSummary)> {
    let adapter = registry::recognize(command, cwd)?;
    let output = RunOutput {
        stdout: &result.stdout,
        stderr: &result.stderr,
        exit_code: result.exit_code,
    };
    Some((adapter, adapter.parse(&output)))
}
