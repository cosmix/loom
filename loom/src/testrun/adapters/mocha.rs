//! Mocha test-runner adapter. Command and output handling use the shared
//! `testrun::recognize` helpers.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{
    distinct_files, package_runner, regex_literal, shell_quote, shell_word, shell_words,
};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

/// Mocha's command, summary parser, and test selector.
pub struct Mocha;

pub static ADAPTER: Mocha = Mocha;

const COMMAND: &[&str] = &["mocha"];

/// Options whose following word is a value rather than a test file.
const VALUE_FLAGS: &[&str] = &[
    "--grep",
    "-g",
    "--fgrep",
    "-f",
    "--reporter",
    "-R",
    "--reporter-option",
    "--reporter-options",
    "--require",
    "-r",
    "--ui",
    "-u",
    "--timeout",
    "-t",
    "--slow",
    "-s",
    "--retries",
    "--config",
    "--package",
    "--extension",
    "--ignore",
    "--file",
    "--node-option",
    "--watch-files",
    "--watch-ignore",
];

/// Mocha prints one final count for each kind of result.
static PASSING: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[ \t]*(\d+) passing\b"));
static FAILING: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[ \t]*(\d+) failing\b"));
static PENDING: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[ \t]*(\d+) pending\b"));

impl TestRunnerAdapter for Mocha {
    fn name(&self) -> &'static str {
        "mocha"
    }

    fn language(&self) -> &'static str {
        "javascript"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A file argument or a grep filter limits the test selection.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let filtered = ["--grep", "-g", "--fgrep", "-f"]
            .iter()
            .any(|flag| has_flag(&args, flag));
        !filtered && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String {
        let runner = package_runner(package_dir);
        let file = shell_word(file);
        let name = shell_quote(&regex_literal(test));
        format!("{runner} mocha {file} --grep {name}")
    }

    /// One invocation runs each selected file; names are covered by their files.
    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        if files.is_empty() {
            return None;
        }
        let runner = package_runner(package_dir);
        Some(format!("{runner} mocha {}", shell_words(files)))
    }

    /// Count tests from Mocha's final summary, including its `0 passing` case.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        summary_from_counts(
            last_match(&PASSING, &text),
            last_match(&FAILING, &text),
            last_match(&PENDING, &text),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};
    use crate::testrun::fixture_support;
    use crate::testrun::{classify, RunOutcome};

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn single_command_uses_npx_without_bun_lock() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        assert_eq!(
            ADAPTER.single_test_command("test/math.js", "math adds", dir.path()),
            "npx mocha test/math.js --grep 'math adds'"
        );
    }

    #[test]
    fn single_command_uses_bunx_with_either_lockfile() {
        for lock in ["bun.lock", "bun.lockb"] {
            let dir = tempfile::tempdir().expect("temporary package directory");
            std::fs::write(dir.path().join(lock), "").expect("write bun lockfile");
            assert_eq!(
                ADAPTER.single_test_command("test/math.js", "math adds", dir.path()),
                "bunx mocha test/math.js --grep 'math adds'"
            );
        }
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let command = ADAPTER.single_test_command("a.test.js", HOSTILE_NAME, dir.path());
        assert_one_word(&command, &regex_literal(HOSTILE_NAME));
    }

    #[test]
    fn command_recognition_accepts_runner_wrappers() {
        for words in [
            &["mocha", "test/math.js"][..],
            &["bunx", "mocha", "test/math.js"],
            &["npx", "mocha", "test/math.js"],
            &["pnpm", "exec", "mocha"],
            &["yarn", "mocha"],
        ] {
            assert!(ADAPTER.recognizes(&argv(words)), "{words:?}");
        }
        for words in [&["echo", "mocha"][..], &["mochawesome"]] {
            assert!(!ADAPTER.recognizes(&argv(words)), "{words:?}");
        }
    }

    #[test]
    fn full_suite_detection_checks_files_and_grep_filters() {
        assert!(ADAPTER.is_full_run(&argv(&["mocha", "--reporter", "spec"])));
        assert!(ADAPTER.is_full_run(&argv(&["bunx", "mocha", "--recursive"])));
        for words in [
            &["mocha", "test/math.js"][..],
            &["mocha", "--grep", "math"],
            &["mocha", "-g", "math"],
            &["mocha", "--fgrep", "math"],
            &["mocha", "-f", "math"],
            &["npx", "mocha", "--grep=math"],
        ] {
            assert!(!ADAPTER.is_full_run(&argv(words)), "{words:?}");
        }
        assert!(!ADAPTER.is_full_run(&argv(&["node", "test/math.js"])));
    }

    #[test]
    fn selection_runs_each_file_once_in_one_command() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let targets = [
            TestTarget {
                file: "a.test.js".into(),
                name: Some("a".into()),
            },
            TestTarget {
                file: "b.test.js".into(),
                name: None,
            },
            TestTarget {
                file: "a.test.js".into(),
                name: Some("other".into()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("npx mocha a.test.js b.test.js")
        );
        std::fs::write(dir.path().join("bun.lock"), "").expect("write bun lockfile");
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("bunx mocha a.test.js b.test.js")
        );
        assert_eq!(ADAPTER.select_command(&[], dir.path()), None);
    }

    #[test]
    fn recorded_outputs_report_counts_and_outcomes() {
        let scenarios = fixture_support::scenarios("mocha");
        assert_eq!(scenarios.len(), 8);
        for scenario in scenarios {
            let (stdout, stderr, exit_code) = fixture_support::load("mocha", &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let (executed, passed, failed, outcome) = match scenario.split('.').next() {
                Some("one-pass") => (Some(1), Some(1), None, RunOutcome::Passed),
                Some("one-fail") => (Some(1), Some(0), Some(1), RunOutcome::Failed),
                Some("no-match") => (Some(0), Some(0), None, RunOutcome::NotSelected),
                Some("suite") => (Some(3), Some(2), Some(1), RunOutcome::Failed),
                other => panic!("unexpected Mocha scenario: {other:?}"),
            };
            assert_eq!(
                (
                    summary.executed,
                    summary.passed,
                    summary.failed,
                    summary.skipped
                ),
                (executed, passed, failed, None),
                "{scenario}"
            );
            assert_eq!(classify(&summary, exit_code), outcome, "{scenario}");
        }
    }

    #[test]
    fn pending_tests_do_not_count_as_executed() {
        let summary = ADAPTER.parse(&RunOutput {
            stdout: "  0 passing (1ms)\n  2 pending\n",
            stderr: "",
            exit_code: Some(0),
        });
        assert_eq!((summary.executed, summary.skipped), (Some(0), Some(2)));
        assert_eq!(classify(&summary, Some(0)), RunOutcome::NotSelected);
    }
}
