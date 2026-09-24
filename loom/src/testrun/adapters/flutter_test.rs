//! `flutter test`: selects Dart test files and reads the last progress counters.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{distinct_files, shell_quote, shell_word, shell_words};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct FlutterTest;

pub static ADAPTER: FlutterTest = FlutterTest;

const COMMAND: &[&str] = &["flutter", "test"];

/// Options with a separate value that must not be mistaken for a test path.
const VALUE_FLAGS: &[&str] = &[
    "--name",
    "--plain-name",
    "-t",
    "--tags",
    "-x",
    "--exclude-tags",
    "--reporter",
    "--platform",
    "--concurrency",
    "--timeout",
    "--test-randomize-ordering-seed",
    "--total-shards",
    "--shard-index",
    "--device-id",
    "--flavor",
    "--dart-define",
    "--dart-define-from-file",
];

/// Options that select a subset of the available tests.
const FILTER_FLAGS: &[&str] = &[
    "--name",
    "--plain-name",
    "-t",
    "--tags",
    "-x",
    "--exclude-tags",
];

/// Progress counters are cumulative. Carriage returns separate the runner's updates.
static PASSED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?:^|[\r\n])\d{2}:\d{2} \+(\d+)(?: -\d+)?(?: ~\d+)?:"));
static FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?:^|[\r\n])\d{2}:\d{2} \+\d+ -(\d+)(?: ~\d+)?:"));
static SKIPPED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?:^|[\r\n])\d{2}:\d{2} \+\d+(?: -\d+)? ~(\d+):"));
static NO_MATCH: LazyLock<Regex> = LazyLock::new(|| pattern(r"No tests match|No tests ran\."));
static BUILD_ERROR: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"Compilation failed|Failed to load|(?m):\d+:\d+: Error:"));

impl TestRunnerAdapter for FlutterTest {
    fn name(&self) -> &'static str {
        "flutter-test"
    }

    fn language(&self) -> &'static str {
        "dart"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A path or a name/tag filter limits the run to a subset of the suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        !FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag))
            && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let file = shell_word(file);
        format!("flutter test {file} --plain-name {}", shell_quote(test))
    }

    /// Flutter accepts several test files in one invocation; names do not
    /// combine reliably across files, so select each distinct file once.
    fn select_command(&self, targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        (!files.is_empty()).then(|| format!("flutter test {}", shell_words(files)))
    }

    /// The last progress counters include completed tests, except that a
    /// failed `loading ... [E]` update records a compiler failure as `-1`.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if NO_MATCH.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        let passed = last_match(&PASSED, &text);
        if passed.unwrap_or(0) == 0 && BUILD_ERROR.is_match(&text) {
            return RunSummary {
                build_failed: true,
                ..RunSummary::default()
            };
        }
        match passed {
            Some(passed) => summary_from_counts(
                Some(passed),
                Some(last_match(&FAILED, &text).unwrap_or(0)),
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
    fn flutter_plain_name_command_selects_one_description() {
        let command =
            ADAPTER.single_test_command("test/fixture_test.dart", "alpha_passes", Path::new("."));
        assert_eq!(
            command,
            "flutter test test/fixture_test.dart --plain-name 'alpha_passes'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_plain_name() {
        let command = ADAPTER.single_test_command("a_test.dart", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, HOSTILE_NAME);
    }

    #[test]
    fn flutter_invocations_match_only_test_subcommand() {
        assert!(ADAPTER.recognizes(&words("flutter test")));
        assert!(ADAPTER.recognizes(&words("VAR=x flutter test test/a_test.dart")));
        assert!(!ADAPTER.recognizes(&words("dart test")));
        assert!(!ADAPTER.recognizes(&words("flutter build")));
    }

    #[test]
    fn flutter_whole_suite_requires_no_selection() {
        assert!(ADAPTER.is_full_run(&words("flutter test --reporter expanded")));
        assert!(!ADAPTER.is_full_run(&words("flutter test test/a_test.dart")));
        for filter in [
            "--name alpha",
            "--plain-name alpha",
            "-t smoke",
            "--tags smoke",
            "-x slow",
            "--exclude-tags slow",
        ] {
            assert!(!ADAPTER.is_full_run(&words(&format!("flutter test {filter}"))));
        }
    }

    #[test]
    fn flutter_selection_uses_each_file_once() {
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
            Some("flutter test test/a_test.dart test/b_test.dart".to_string())
        );
        assert_eq!(ADAPTER.select_command(&[], Path::new(".")), None);
    }

    #[test]
    fn flutter_recordings_produce_expected_outcomes() {
        let recorded = scenarios(ADAPTER.name());
        assert!(!recorded.is_empty(), "flutter-test fixtures are missing");
        for scenario in recorded {
            let (stdout, stderr, exit_code) = load(ADAPTER.name(), &scenario);
            let output = RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            };
            let summary = ADAPTER.parse(&output);
            let expected = match scenario.split('.').next() {
                Some("one-pass") => RunOutcome::Passed,
                Some("one-fail" | "suite") => RunOutcome::Failed,
                Some("no-match") => RunOutcome::NotSelected,
                Some("build-error") => RunOutcome::BuildFailed,
                _ => panic!("unknown flutter-test fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
