//! `sbt`: recognises test tasks and reads their final test summary.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::shell_quote;
use crate::testrun::recognize::{combined_output, command_args, last_match, pattern, positionals};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Sbt;

pub static ADAPTER: Sbt = Sbt;

const COMMAND: &[&str] = &["sbt"];

/// sbt's final `Passed:` or `Failed:` line reports one suite-wide total.
static TOTAL: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:info|error)\] (?:Passed|Failed): Total (\d+), Failed \d+, Errors \d+, Passed \d+\s*$",
    )
});
static FAILED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:info|error)\] (?:Passed|Failed): Total \d+, Failed (\d+), Errors \d+, Passed \d+\s*$",
    )
});
static ERRORS: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:info|error)\] (?:Passed|Failed): Total \d+, Failed \d+, Errors (\d+), Passed \d+\s*$",
    )
});
static PASSED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:info|error)\] (?:Passed|Failed): Total \d+, Failed \d+, Errors \d+, Passed (\d+)\s*$",
    )
});
static NO_MATCH: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\b(?:No tests to run|No tests were executed)\b"));
static BUILD_ERROR: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\bCompilation failed\b"));

/// Task words can be separate arguments or one quoted sbt command.
fn task_word(word: &str) -> &str {
    word.split_whitespace().next().unwrap_or(word)
}

fn is_test_task(word: &str) -> bool {
    let task = task_word(word);
    task.starts_with("test")
        || task.ends_with("/test")
        || task.ends_with("/testOnly")
        || task.ends_with("/testQuick")
}

fn is_full_task(word: &str) -> bool {
    matches!(task_word(word), "test") || task_word(word).ends_with("/test")
}

fn is_filtered_task(word: &str) -> bool {
    let task = task_word(word);
    task.starts_with("testOnly")
        || task.starts_with("testQuick")
        || task.ends_with("/testOnly")
        || task.ends_with("/testQuick")
}

impl TestRunnerAdapter for Sbt {
    fn name(&self) -> &'static str {
        "sbt"
    }

    fn language(&self) -> &'static str {
        "scala"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some_and(|args| {
            positionals(&args, &[])
                .iter()
                .any(|word| is_test_task(word))
        })
    }

    /// A plain `test` task runs the whole suite; `testOnly` and `testQuick`
    /// select a subset, including when quoted into one sbt command word.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let tasks = positionals(&args, &[]);
        tasks.iter().any(|word| is_full_task(word))
            && !tasks.iter().any(|word| is_filtered_task(word))
    }

    fn single_test_command(&self, _file: &str, test: &str, _package_dir: &Path) -> String {
        format!("sbt {}", shell_quote(&format!("testOnly {test}")))
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// The final summary has no skipped count. A no-test message establishes
    /// zero executed tests even though sbt exits successfully in that case.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if let Some(total) = last_match(&TOTAL, &text) {
            let failed = last_match(&FAILED, &text).unwrap_or(0);
            let errors = last_match(&ERRORS, &text).unwrap_or(0);
            return RunSummary {
                executed: Some(total),
                passed: last_match(&PASSED, &text),
                failed: Some(failed.saturating_add(errors)),
                ..RunSummary::default()
            };
        }
        if BUILD_ERROR.is_match(&text) {
            return RunSummary {
                build_failed: true,
                ..RunSummary::default()
            };
        }
        if NO_MATCH.is_match(&text) {
            return RunSummary {
                executed: Some(0),
                passed: Some(0),
                failed: Some(0),
                ..RunSummary::default()
            };
        }
        RunSummary::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};
    use crate::testrun::fixture_support::{load, scenarios};
    use crate::testrun::{classify, RunOutcome};

    fn words(command: &str) -> Vec<String> {
        command.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn named_suite_uses_quoted_testonly_task() {
        let command = ADAPTER.single_test_command(
            "src/test/PassSpec.scala",
            "example.PassSpec",
            Path::new("."),
        );
        assert_eq!(command, "sbt 'testOnly example.PassSpec'");
    }

    #[test]
    fn hostile_test_name_stays_in_one_sbt_command() {
        let command = ADAPTER.single_test_command("A.scala", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &format!("testOnly {HOSTILE_NAME}"));
    }

    #[test]
    fn sbt_test_tasks_are_recognized() {
        assert!(ADAPTER.recognizes(&words("sbt test")));
        assert!(ADAPTER.recognizes(&["sbt".into(), "testOnly example.PassSpec".into()]));
        assert!(ADAPTER.recognizes(&words("sbt core/test")));
        assert!(ADAPTER.recognizes(&words("sbt core/testQuick example.PassSpec")));
        assert!(!ADAPTER.recognizes(&words("sbt compile")));
    }

    #[test]
    fn only_unfiltered_test_tasks_are_full_runs() {
        assert!(ADAPTER.is_full_run(&words("sbt test")));
        assert!(ADAPTER.is_full_run(&words("sbt core/test")));
        assert!(!ADAPTER.is_full_run(&words("sbt testOnly example.PassSpec")));
        assert!(!ADAPTER.is_full_run(&["sbt".into(), "testQuick example.PassSpec".into()]));
        assert!(!ADAPTER.is_full_run(&words("sbt test core/testOnly example.PassSpec")));
        assert!(!ADAPTER.is_full_run(&words("sbt compile")));
    }

    #[test]
    fn sbt_has_no_multi_target_selection_command() {
        let target = TestTarget {
            file: "src/test/PassSpec.scala".into(),
            name: Some("example.PassSpec".into()),
        };
        assert_eq!(ADAPTER.select_command(&[target], Path::new(".")), None);
    }

    #[test]
    fn sbt_errors_count_as_failed_tests() {
        let output = RunOutput {
            stdout: "[error] Failed: Total 3, Failed 1, Errors 1, Passed 1\n",
            stderr: "",
            exit_code: Some(1),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!((summary.executed, summary.failed), (Some(3), Some(2)));
    }

    #[test]
    fn sbt_alternate_empty_run_message_records_zero() {
        let output = RunOutput {
            stdout: "[info] No tests were executed\n",
            stderr: "",
            exit_code: Some(0),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!(summary.executed, Some(0));
    }

    #[test]
    fn documented_sbt_scenarios_parse_and_classify() {
        let recorded = scenarios(ADAPTER.name());
        assert_eq!(recorded.len(), 5, "expected five sbt scenarios");
        for scenario in recorded {
            let (stdout, stderr, exit_code) = load(ADAPTER.name(), &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let expected = match scenario.as_str() {
                "one-pass" => RunOutcome::Passed,
                "one-fail" | "suite" => RunOutcome::Failed,
                "no-match" => RunOutcome::NotSelected,
                "build-error" => RunOutcome::BuildFailed,
                other => panic!("unexpected sbt scenario: {other}"),
            };
            assert_eq!(
                classify(&summary, exit_code),
                expected,
                "{scenario}: {summary:?}"
            );
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
