//! `bun test` command selection and result-summary parsing.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{distinct_files, regex_literal, shell_quote, shell_word, shell_words};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

/// The Bun built-in test runner.
pub struct BunTest;

pub static ADAPTER: BunTest = BunTest;

const COMMAND: &[&str] = &["bun", "test"];

/// Bun options whose following word is a value rather than a test path.
const VALUE_FLAGS: &[&str] = &[
    "-t",
    "--test-name-pattern",
    "--timeout",
    "--preload",
    "--root",
    "--coverage-reporter",
    "--coverage-dir",
    "--reporter",
    "--reporter-outfile",
];

/// Bun prints these aggregate lines on stderr, including zero counts.
static PASSED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[ \t]*(\d+)[ \t]+pass\b"));
static FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[ \t]*(\d+)[ \t]+fail\b"));
static SKIPPED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[ \t]*(\d+)[ \t]+skip(?:ped)?\b"));
static NO_MATCH: LazyLock<Regex> = LazyLock::new(|| pattern(r"matched 0 tests\b"));

impl TestRunnerAdapter for BunTest {
    fn name(&self) -> &'static str {
        "bun-test"
    }

    fn language(&self) -> &'static str {
        "javascript"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A path operand or a name-pattern option selects only part of the suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        !has_flag(&args, "-t")
            && !has_flag(&args, "--test-name-pattern")
            && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let file = shell_word(file);
        let name = shell_quote(&regex_literal(test));
        format!("bun test {file} -t {name}")
    }

    /// One invocation runs every distinct selected file.
    fn select_command(&self, targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        if files.is_empty() {
            return None;
        }
        Some(format!("bun test {}", shell_words(files)))
    }

    /// Count tests from Bun's aggregate lines; a regex with no matches ran zero.
    /// `filtered out` does not count as a skipped or executed test.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if NO_MATCH.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        let mut summary = summary_from_counts(
            last_match(&PASSED, &text),
            last_match(&FAILED, &text),
            last_match(&SKIPPED, &text),
        );
        if summary.executed.is_none() && summary.skipped.is_some() {
            summary.executed = Some(0);
        }
        summary
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
    fn command_selects_a_full_bun_test_name() {
        let command = ADAPTER.single_test_command(
            "tests/math.test.ts",
            "math adds two numbers",
            Path::new("."),
        );
        assert_eq!(
            command,
            "bun test tests/math.test.ts -t 'math adds two numbers'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("a.test.ts", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &regex_literal(HOSTILE_NAME));
    }

    #[test]
    fn identifies_direct_and_environment_prefixed_bun_tests() {
        for command in [
            "bun test",
            "CI=1 bun test tests/math.test.ts",
            "env CI=1 bun test",
        ] {
            assert!(ADAPTER.recognizes(&words(command)), "{command}");
        }
        for command in [
            "bun run build",
            "bun run test",
            "bunx vitest",
            "echo bun test",
        ] {
            assert!(!ADAPTER.recognizes(&words(command)), "{command}");
        }
    }

    #[test]
    fn suite_detection_checks_file_and_name_filters() {
        for command in ["bun test", "env CI=1 bun test --timeout 1000"] {
            assert!(ADAPTER.is_full_run(&words(command)), "{command}");
        }
        for command in [
            "bun test tests/math.test.ts",
            "bun test -t adds",
            "bun test --test-name-pattern=adds",
            "bun run build",
        ] {
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn selection_runs_each_file_once() {
        let targets = [
            TestTarget {
                file: "a.test.ts".into(),
                name: Some("alpha".into()),
            },
            TestTarget {
                file: "b.test.ts".into(),
                name: None,
            },
            TestTarget {
                file: "a.test.ts".into(),
                name: Some("beta".into()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, Path::new(".")).as_deref(),
            Some("bun test a.test.ts b.test.ts")
        );
        assert_eq!(ADAPTER.select_command(&[], Path::new(".")), None);
    }

    #[test]
    fn recorded_bun_runs_have_the_expected_outcomes() {
        let scenarios = fixture_support::scenarios(ADAPTER.name());
        assert!(!scenarios.is_empty(), "missing bun-test fixtures");
        for scenario in scenarios {
            let (stdout, stderr, exit_code) = fixture_support::load(ADAPTER.name(), &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let base = scenario.split('.').next().unwrap_or(&scenario);
            let expected = match base {
                "one-pass" => RunOutcome::Passed,
                "one-fail" | "suite" => RunOutcome::Failed,
                "no-match" => RunOutcome::NotSelected,
                other => panic!("unexpected bun-test fixture: {other}"),
            };
            assert_eq!(
                classify(&summary, exit_code),
                expected,
                "{scenario}: {summary:?}"
            );
            if base == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
