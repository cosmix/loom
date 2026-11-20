//! What the acceptance runner keeps from each criterion it ran: results,
//! failure messages, warnings for suspicious stderr, and the tests a
//! timed-out criterion left running.

use std::path::Path;

use super::result::{AcceptanceResult, CriterionResult};
use crate::testrun::{registry, RunOutput};

#[derive(Default)]
pub(super) struct CollectedResults {
    results: Vec<CriterionResult>,
    failures: Vec<String>,
}

pub(super) struct EvaluatedCriterion {
    pub(super) result: CriterionResult,
    pub(super) failures: Vec<String>,
}

impl CollectedResults {
    pub(super) fn push(&mut self, evaluated: EvaluatedCriterion) {
        self.results.push(evaluated.result);
        self.failures.extend(evaluated.failures);
    }

    pub(super) fn warn_on_suspicious_stderr(&self) {
        for result in &self.results {
            for warning in detect_stderr_warnings(result) {
                eprintln!("warning: {warning}");
            }
        }
    }

    pub(super) fn finish(self) -> AcceptanceResult {
        if self.failures.is_empty() {
            AcceptanceResult::AllPassed {
                results: self.results,
            }
        } else {
            AcceptanceResult::Failed {
                results: self.results,
                failures: self.failures,
            }
        }
    }
}

/// Record on a timed-out `result` the tests its output shows started and not
/// finished; `command` is the criterion's expanded command, run in `cwd`.
pub(super) fn note_running_tests(result: &mut CriterionResult, command: &str, cwd: &Path) {
    if !result.timed_out {
        return;
    }
    let out = RunOutput {
        stdout: &result.stdout,
        stderr: &result.stderr,
        exit_code: result.exit_code,
    };
    result.running_at_timeout = registry::unfinished_tests(command, cwd, &out);
}

fn detect_stderr_warnings(result: &CriterionResult) -> Vec<String> {
    if !result.success || result.stderr.is_empty() || result.cached {
        return Vec::new();
    }
    let patterns = [
        "connection refused",
        "permission denied",
        "failed to download",
        "blocked",
        "EACCES",
        "ECONNREFUSED",
        "unable to connect",
        "network error",
        "sandbox",
    ];
    let stderr = result.stderr.to_lowercase();
    patterns
        .iter()
        .filter(|pattern| stderr.contains(pattern.to_lowercase().as_str()))
        .map(|pattern| {
            format!(
                "Command '{}' succeeded (exit 0) but stderr contains '{}' — may indicate a silent failure",
                result.command, pattern
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::RunningTests;
    use std::time::Duration;

    fn make_result(success: bool, stderr: &str) -> CriterionResult {
        CriterionResult::new(
            "test-command".to_string(),
            success,
            String::new(),
            stderr.to_string(),
            if success { Some(0) } else { Some(1) },
            Duration::from_millis(100),
            false,
        )
    }

    #[test]
    fn stderr_warning_detection_is_case_insensitive_and_success_only() {
        let warning = make_result(true, "BLOCKED by firewall, connection refused");
        let failure = make_result(false, "connection refused");
        assert_eq!(detect_stderr_warnings(&warning).len(), 2);
        assert!(detect_stderr_warnings(&failure).is_empty());
    }

    #[test]
    fn ordinary_success_stderr_is_not_a_warning() {
        let result = make_result(true, "Compiling project\nFinished dev target");
        assert!(detect_stderr_warnings(&result).is_empty());
    }

    #[test]
    fn only_a_timeout_records_running_tests() {
        let stdout = "running 2 tests\ntest quick ... ok\n\
            test stuck has been running for over 60 seconds\n";
        let mut result = make_result(false, "");
        result.stdout = stdout.to_string();
        note_running_tests(&mut result, "cargo test", Path::new("."));
        assert_eq!(result.running_at_timeout, None);
        result.timed_out = true;
        note_running_tests(&mut result, "make check", Path::new("."));
        let stuck = RunningTests::Named(vec!["stuck".to_string()]);
        assert_eq!(result.running_at_timeout, Some(stuck));
        result.stdout = "building".to_string();
        note_running_tests(&mut result, "cargo test", Path::new("."));
        assert_eq!(result.running_at_timeout, Some(RunningTests::Unknown));
        note_running_tests(&mut result, "make build", Path::new("."));
        assert_eq!(result.running_at_timeout, None, "no test run");
    }
}
