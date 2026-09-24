//! Node's built-in test runner and its TAP or spec reporter trailer.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{distinct_files, regex_literal, shell_quote, shell_word, shell_words};
use crate::testrun::recognize::{
    combined_output, command_args, count_matches, has_flag, last_match, pattern, positionals,
    summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct NodeTest;

pub static ADAPTER: NodeTest = NodeTest;

const COMMAND: &[&str] = &["node"];

/// Node options whose following word is a value, not a test file.
const VALUE_FLAGS: &[&str] = &[
    "--test-name-pattern",
    "--test-skip-pattern",
    "--test-file-pattern",
    "--test-reporter",
    "--test-reporter-destination",
    "--test-concurrency",
    "--test-timeout",
    "--test-isolation",
    "--require",
    "-r",
    "--import",
    "--loader",
    "--experimental-loader",
    "--env-file",
    "--conditions",
];

/// Both Node's TAP and spec reporters end in these count lines.
static TESTS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^(?:#|ℹ) tests (\d+)\s*$"));
static PASSED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^(?:#|ℹ) pass (\d+)\s*$"));
static FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^(?:#|ℹ) fail (\d+)\s*$"));
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^(?:#|ℹ) skipped (\d+)\s*$"));
static EMPTY_PLAN: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^1\.\.0\r?$"));

impl TestRunnerAdapter for NodeTest {
    fn name(&self) -> &'static str {
        "node-test"
    }

    fn language(&self) -> &'static str {
        "javascript"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some_and(|args| has_flag(&args, "--test"))
    }

    /// A name, skip, only, file pattern, or file operand narrows the run.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let filtered = [
            "--test-name-pattern",
            "--test-skip-pattern",
            "--test-only",
            "--test-file-pattern",
        ]
        .iter()
        .any(|flag| has_flag(&args, flag));
        has_flag(&args, "--test") && !filtered && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let name = shell_quote(&format!("^{}$", regex_literal(test)));
        let file = shell_word(file);
        format!("node --test --test-name-pattern={name} {file}")
    }

    /// One invocation can run all selected files; duplicate files add no tests.
    fn select_command(&self, targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        if files.is_empty() {
            return None;
        }
        Some(format!("node --test {}", shell_words(files)))
    }

    /// A zero-test TAP file still contributes one successful file wrapper to
    /// Node's trailer. Remove those wrappers before counting executed tests.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if last_match(&TESTS, &text).is_none() {
            return RunSummary::default();
        }
        let empty_files = count_matches(&EMPTY_PLAN, &text);
        let passed = last_match(&PASSED, &text).unwrap_or(0);
        summary_from_counts(
            Some(passed.saturating_sub(empty_files)),
            Some(last_match(&FAILED, &text).unwrap_or(0)),
            Some(last_match(&SKIPPED, &text).unwrap_or(0)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};
    use crate::testrun::{classify, fixture_support, RunOutcome};

    fn words(command: &str) -> Vec<String> {
        command.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn command_filters_the_full_test_name() {
        let command = ADAPTER.single_test_command("test/a.test.js", "suite alpha", Path::new("."));
        assert_eq!(
            command,
            "node --test --test-name-pattern='^suite alpha$' test/a.test.js"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("a.test.js", HOSTILE_NAME, Path::new("."));
        let option = format!("--test-name-pattern=^{}$", regex_literal(HOSTILE_NAME));
        assert_one_word(&command, &option);
    }

    #[test]
    fn identifies_node_test_invocations() {
        for command in [
            "node --test",
            "env MODE=ci node --test test/a.test.js",
            "MODE=ci node --test --test-reporter=spec",
        ] {
            assert!(ADAPTER.recognizes(&words(command)), "{command}");
        }
        for command in ["node", "node script.js", "echo node --test", "bun test"] {
            assert!(!ADAPTER.recognizes(&words(command)), "{command}");
        }
    }

    #[test]
    fn distinguishes_complete_and_targeted_runs() {
        for command in [
            "node --test",
            "node --test --test-reporter spec",
            "env MODE=ci node --test --test-concurrency 4",
        ] {
            assert!(ADAPTER.is_full_run(&words(command)), "{command}");
        }
        for command in [
            "node --test test/a.test.js",
            "node --test --test-name-pattern alpha",
            "node --test --test-name-pattern=alpha",
            "node --test --test-skip-pattern alpha",
            "node --test --test-skip-pattern=alpha",
            "node --test --test-only",
            "node --test-only --test",
            "node script.js",
        ] {
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn selection_lists_each_file_once() {
        let targets = [
            TestTarget {
                file: "test/a.test.js".into(),
                name: Some("alpha".into()),
            },
            TestTarget {
                file: "test/b.test.js".into(),
                name: None,
            },
            TestTarget {
                file: "test/a.test.js".into(),
                name: Some("beta".into()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, Path::new(".")),
            Some("node --test test/a.test.js test/b.test.js".into())
        );
        assert_eq!(ADAPTER.select_command(&[], Path::new(".")), None);
    }

    #[test]
    fn fixture_outcomes_reflect_executed_tests() {
        for scenario in fixture_support::scenarios("node-test") {
            let (stdout, stderr, exit_code) = fixture_support::load("node-test", &scenario);
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
                other => panic!("unexpected node-test fixture: {other:?}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
