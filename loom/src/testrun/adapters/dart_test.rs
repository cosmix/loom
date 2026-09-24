//! `dart test`: selects Dart test files and reads the final progress counters.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{distinct_files, shell_quote, shell_word, shell_words};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct DartTest;

pub static ADAPTER: DartTest = DartTest;

const COMMAND: &[&str] = &["dart", "test"];
const RUN_COMMAND: &[&str] = &["dart", "run", "test"];

/// Options with a separate value word; those values are not file targets.
const VALUE_FLAGS: &[&str] = &[
    "-n",
    "--name",
    "-N",
    "--plain-name",
    "-t",
    "--tags",
    "-x",
    "--exclude-tags",
    "-p",
    "--platform",
    "-r",
    "--reporter",
    "-j",
    "--concurrency",
    "--timeout",
    "--retry",
    "--test-randomize-ordering-seed",
    "--total-shards",
    "--shard-index",
    "--file-reporter",
    "--file-reporter-name",
    "--file-reporter-output",
];

const FILTER_FLAGS: &[&str] = &[
    "-n",
    "--name",
    "-N",
    "--plain-name",
    "-t",
    "--tags",
    "-x",
    "--exclude-tags",
];

/// The compact reporter redraws progress with carriage returns, and may use ANSI colors.
static PASSED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)(?:^|\r)\d{2}:\d{2}(?::\d{2})? \+(\d+)(?: -\d+)?(?: ~\d+)?:"));
static FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)(?:^|\r)\d{2}:\d{2}(?::\d{2})? \+\d+ -(\d+)(?: ~\d+)?:"));
static SKIPPED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)(?:^|\r)\d{2}:\d{2}(?::\d{2})? \+\d+(?: -\d+)? ~(\d+):"));
static NO_MATCH: LazyLock<Regex> = LazyLock::new(|| pattern(r"No tests match\b|No tests ran\."));
static BUILD_ERROR: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"Failed to load |\S+\.dart:\d+:\d+: Error:"));

fn test_args(argv: &[String]) -> Option<Vec<String>> {
    command_args(argv, COMMAND).or_else(|| command_args(argv, RUN_COMMAND))
}

impl TestRunnerAdapter for DartTest {
    fn name(&self) -> &'static str {
        "dart-test"
    }

    fn language(&self) -> &'static str {
        "dart"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        test_args(argv).is_some()
    }

    /// A path operand or a name/tag filter narrows the suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = test_args(argv) else {
            return false;
        };
        !FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag))
            && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let file = shell_word(file);
        format!("dart test {file} --plain-name {}", shell_quote(test))
    }

    /// Run each distinct file once; Dart's file selection includes all tests in it.
    fn select_command(&self, targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        (!files.is_empty()).then(|| format!("dart test {}", shell_words(files)))
    }

    /// Use the last progress counts, not the exit code or number of redraws.
    /// A failed load can show `+0 -1`; it did not execute a test.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let passed = last_match(&PASSED, &text);
        let failed = last_match(&FAILED, &text);
        if BUILD_ERROR.is_match(&text) && passed.unwrap_or(0) == 0 && failed.unwrap_or(0) <= 1 {
            return RunSummary {
                build_failed: true,
                ..RunSummary::default()
            };
        }
        if NO_MATCH.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        match passed {
            Some(passed) => summary_from_counts(
                Some(passed),
                Some(failed.unwrap_or(0)),
                last_match(&SKIPPED, &text),
            ),
            None => RunSummary::default(),
        }
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
    fn dart_description_command_targets_one_file() {
        let command =
            ADAPTER.single_test_command("test/fixture_test.dart", "alpha passes", Path::new("."));
        assert_eq!(
            command,
            "dart test test/fixture_test.dart --plain-name 'alpha passes'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_plain_name() {
        let command = ADAPTER.single_test_command("a_test.dart", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, HOSTILE_NAME);
    }

    #[test]
    fn dart_invocation_variants_are_identified() {
        assert!(ADAPTER.recognizes(&words("dart test")));
        assert!(ADAPTER.recognizes(&words("dart run test")));
        assert!(ADAPTER.recognizes(&words("env VAR=x dart test")));
        assert!(!ADAPTER.recognizes(&words("flutter test")));
        assert!(!ADAPTER.recognizes(&words("dart analyze")));
    }

    #[test]
    fn dart_filters_and_file_operands_narrow_the_run() {
        assert!(ADAPTER.is_full_run(&words("dart test --reporter expanded")));
        assert!(ADAPTER.is_full_run(&words("dart run test --no-color")));
        for command in [
            "dart test test/fixture_test.dart",
            "dart test --name alpha",
            "dart test -n alpha",
            "dart test --plain-name alpha",
            "dart test -N alpha",
            "dart test --tags slow",
            "dart test -t slow",
            "dart test --exclude-tags slow",
            "dart test -x slow",
        ] {
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn dart_selection_runs_each_file_once() {
        let targets = [
            TestTarget {
                file: "test/a_test.dart".to_string(),
                name: Some("alpha".to_string()),
            },
            TestTarget {
                file: "test/b_test.dart".to_string(),
                name: None,
            },
            TestTarget {
                file: "test/a_test.dart".to_string(),
                name: Some("beta".to_string()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, Path::new(".")),
            Some("dart test test/a_test.dart test/b_test.dart".to_string())
        );
        assert_eq!(ADAPTER.select_command(&[], Path::new(".")), None);
    }

    #[test]
    fn dart_fixture_outputs_classify_with_final_counts() {
        let recorded = scenarios(ADAPTER.name());
        assert!(!recorded.is_empty(), "dart-test fixtures are missing");
        for scenario in recorded {
            let (stdout, stderr, exit_code) = load(ADAPTER.name(), &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let expected = match scenario.split('.').next() {
                Some("one-pass") => RunOutcome::Passed,
                Some("one-fail" | "suite") => RunOutcome::Failed,
                Some("no-match") => RunOutcome::NotSelected,
                Some("build-error") => RunOutcome::BuildFailed,
                _ => panic!("unknown dart-test fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }

    #[test]
    fn skipped_progress_does_not_count_as_executed() {
        let summary = ADAPTER.parse(&RunOutput {
            stdout: "\r00:00 +1 -1 ~2: Some tests failed.\n",
            stderr: "",
            exit_code: Some(1),
        });
        assert_eq!((summary.executed, summary.skipped), (Some(2), Some(2)));
    }
}
