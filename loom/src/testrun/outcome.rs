//! Turning a parsed run into a verdict.

use super::{RunOutcome, RunSummary};

/// The verdict on a run from its parsed summary and exit code, first rule that
/// applies:
///
/// 1. `build_failed` ⇒ `BuildFailed`;
/// 2. `executed == Some(0)` ⇒ `NotSelected`;
/// 3. `failed > 0`, or at least one test ran and the exit code is not 0 ⇒ `Failed`;
/// 4. at least one test ran, none failed, exit code 0 ⇒ `Passed`;
/// 5. otherwise nothing was parsed ⇒ `Unparsed`.
///
/// A missing exit code (killed by a signal) counts as non-zero.
pub fn classify(summary: &RunSummary, exit_code: Option<i32>) -> RunOutcome {
    if summary.build_failed {
        return RunOutcome::BuildFailed;
    }
    if summary.executed == Some(0) {
        return RunOutcome::NotSelected;
    }
    let ran = summary.executed.is_some_and(|executed| executed >= 1);
    let failed = summary.failed.unwrap_or(0);
    let exited_zero = exit_code == Some(0);
    if failed > 0 || (ran && !exited_zero) {
        RunOutcome::Failed
    } else if ran && exited_zero {
        RunOutcome::Passed
    } else {
        RunOutcome::Unparsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ran(executed: u64, failed: u64) -> RunSummary {
        RunSummary {
            executed: Some(executed),
            passed: Some(executed - failed),
            failed: Some(failed),
            ..RunSummary::default()
        }
    }

    #[test]
    fn classify_build_failure_wins() {
        let summary = RunSummary {
            build_failed: true,
            ..ran(3, 1)
        };
        assert_eq!(classify(&summary, Some(101)), RunOutcome::BuildFailed);
    }

    #[test]
    fn classify_zero_executed_is_not_selected() {
        assert_eq!(classify(&ran(0, 0), Some(0)), RunOutcome::NotSelected);
        assert_eq!(classify(&ran(0, 0), Some(1)), RunOutcome::NotSelected);
    }

    #[test]
    fn classify_failed_count_fails() {
        assert_eq!(classify(&ran(3, 1), Some(101)), RunOutcome::Failed);
        assert_eq!(classify(&ran(3, 1), Some(0)), RunOutcome::Failed);
        let only_failures = RunSummary {
            failed: Some(2),
            ..RunSummary::default()
        };
        assert_eq!(classify(&only_failures, Some(1)), RunOutcome::Failed);
    }

    #[test]
    fn classify_nonzero_exit_after_tests_ran_fails() {
        assert_eq!(classify(&ran(2, 0), Some(1)), RunOutcome::Failed);
        assert_eq!(classify(&ran(2, 0), None), RunOutcome::Failed);
    }

    #[test]
    fn classify_clean_run_passes() {
        assert_eq!(classify(&ran(1, 0), Some(0)), RunOutcome::Passed);
    }

    #[test]
    fn classify_without_counts_is_unparsed() {
        let nothing = RunSummary::default();
        assert_eq!(classify(&nothing, Some(0)), RunOutcome::Unparsed);
        assert_eq!(classify(&nothing, Some(2)), RunOutcome::Unparsed);
        let passed_only = RunSummary {
            passed: Some(3),
            ..RunSummary::default()
        };
        assert_eq!(classify(&passed_only, Some(0)), RunOutcome::Unparsed);
    }
}
