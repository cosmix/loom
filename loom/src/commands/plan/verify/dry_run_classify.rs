//! How `loom plan verify --dry-run` reads one criterion result: what kind of
//! outcome it is, how serious that is for a stage of a given shape, and when a
//! first timeout earns a second run.

use crate::verify::criteria::CriterionResult;

/// What one criterion did at the base commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryOutcome {
    /// The sandbox, network or disk refused it; the text is the marker found.
    Environment(String),
    /// The command could not start (exit 126 or 127); the text is `exit <code>`.
    Missing(String),
    TimedOut,
    /// The test runner selected no test.
    ZeroTests,
    Passed,
    /// A red criterion: the expected state before the stage's work.
    Failed,
}

/// How a finding is reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrySeverity {
    Error,
    /// Never returned by [`severity`]: the baseline warning for a stage that
    /// cannot tell done from not done is pushed by the caller from
    /// [`stage_cannot_tell`].
    Warning,
    Note,
}

/// Output text that means the environment, not the criterion, failed.
const ENVIRONMENT_MARKERS: [&str; 9] = [
    "bwrap:",
    "operation not permitted",
    "temporary failure in name resolution",
    "could not resolve host",
    "couldn't resolve host",
    "network is unreachable",
    "connection refused",
    "read-only file system",
    "no space left on device",
];

/// The zero-test guard's failure text (`verify::criteria::zero_tests`).
const ZERO_TESTS: &str = "selected zero tests";

/// Classify `result` and the `failures` the runner reported for it, assuming
/// the run printed no test summary.
pub fn classify(result: &CriterionResult, failures: &[String]) -> DryOutcome {
    classify_with(result, failures, false)
}

/// [`classify`] with `has_summary`: a test runner printed its summary, so
/// words in the output such as `operation not permitted` are an assertion's
/// own text and name no environment fault.
pub(super) fn classify_with(
    result: &CriterionResult,
    failures: &[String],
    has_summary: bool,
) -> DryOutcome {
    if result.timed_out {
        return DryOutcome::TimedOut;
    }
    if failures.iter().any(|failure| failure.contains(ZERO_TESTS)) {
        return DryOutcome::ZeroTests;
    }
    let failed_run = !result.success && result.exit_code.is_some_and(|code| code != 0);
    if failed_run && !has_summary {
        if let Some(marker) = environment_marker(result) {
            return DryOutcome::Environment(marker.to_string());
        }
    }
    if let Some(code @ (126 | 127)) = result.exit_code {
        return DryOutcome::Missing(format!("exit {code}"));
    }
    if result.success && failures.is_empty() {
        DryOutcome::Passed
    } else {
        DryOutcome::Failed
    }
}

fn environment_marker(result: &CriterionResult) -> Option<&'static str> {
    let output = format!("{}\n{}", result.stderr, result.stdout).to_lowercase();
    ENVIRONMENT_MARKERS
        .into_iter()
        .find(|marker| output.contains(marker))
}

/// How serious `outcome` is for a stage with or without dependencies.
pub fn severity(outcome: &DryOutcome, stage_has_dependencies: bool) -> Option<DrySeverity> {
    match outcome {
        DryOutcome::Environment(_) | DryOutcome::TimedOut => Some(DrySeverity::Error),
        // A filter naming tests the stage itself or an earlier stage adds
        // selects nothing at base by design; the static Rust filter lint and
        // completion's zero-test guard cover a filter that stays empty.
        DryOutcome::ZeroTests | DryOutcome::Failed => Some(DrySeverity::Note),
        // An earlier stage may provide the program or script.
        DryOutcome::Missing(_) if stage_has_dependencies => Some(DrySeverity::Note),
        DryOutcome::Missing(_) => Some(DrySeverity::Error),
        DryOutcome::Passed if stage_has_dependencies => None,
        DryOutcome::Passed => Some(DrySeverity::Note),
    }
}

/// Whether a stage cannot tell done from not done: it has no dependencies and
/// every one of its acceptance criteria already passes at base. One passing
/// criterion among failing ones is never a finding by itself.
pub fn stage_cannot_tell(acceptance: &[DryOutcome], stage_has_dependencies: bool) -> bool {
    !stage_has_dependencies
        && !acceptance.is_empty()
        && acceptance
            .iter()
            .all(|outcome| *outcome == DryOutcome::Passed)
}

/// `first`, or the outcome of one more run when `first` timed out. The rerun
/// shares the first run's checkout, whose build cache the first run warmed, so
/// only a command that is slow with a warm cache reads as a timeout.
pub fn settle_timeout(first: DryOutcome, rerun: &mut dyn FnMut() -> DryOutcome) -> DryOutcome {
    if first == DryOutcome::TimedOut {
        rerun()
    } else {
        first
    }
}
