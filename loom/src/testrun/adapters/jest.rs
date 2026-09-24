//! Jest's command selection and `Tests:` summary format.

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

pub struct Jest;

pub static ADAPTER: Jest = Jest;

const COMMAND: &[&str] = &["jest"];

/// Jest options whose following word is a value, not a test file.
const VALUE_FLAGS: &[&str] = &[
    "-t",
    "--testNamePattern",
    "--testPathPattern",
    "--testPathPatterns",
    "--config",
    "--coverageDirectory",
    "--maxWorkers",
    "--testEnvironment",
    "--rootDir",
    "--projects",
    "--selectProjects",
    "--testRegex",
    "--testMatch",
    "--outputFile",
    "--reporters",
    "--shard",
];

/// The final Jest test summary, which may appear on stderr.
static TESTS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^Tests:[ \t]*([^\r\n]*)"));
static TOTAL: LazyLock<Regex> = LazyLock::new(|| pattern(r"(\d+) total\b"));
static PASSED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(\d+) passed\b"));
static FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(\d+) failed\b"));
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(\d+) skipped\b"));

impl TestRunnerAdapter for Jest {
    fn name(&self) -> &'static str {
        "jest"
    }

    fn language(&self) -> &'static str {
        "javascript"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A file operand or test-name/path pattern selects a subset of tests.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let filtered = [
            "-t",
            "--testNamePattern",
            "--testPathPattern",
            "--testPathPatterns",
        ]
        .iter()
        .any(|flag| has_flag(&args, flag));
        !filtered && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String {
        let runner = package_runner(package_dir);
        let file = shell_word(file);
        let name = shell_quote(&regex_literal(test));
        format!("{runner} jest {file} -t {name}")
    }

    /// Jest accepts all selected files as operands of one invocation.
    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        if files.is_empty() {
            return None;
        }
        let runner = package_runner(package_dir);
        Some(format!("{runner} jest {}", shell_words(files)))
    }

    /// Missing categories on a valid `Tests:` line have zero tests. In
    /// particular, an all-skipped line reports that no tests executed.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let Some(line) = TESTS
            .captures_iter(&text)
            .last()
            .and_then(|capture| capture.get(1))
        else {
            return RunSummary::default();
        };
        if last_match(&TOTAL, line.as_str()).is_none() {
            return RunSummary::default();
        }
        summary_from_counts(
            Some(last_match(&PASSED, line.as_str()).unwrap_or(0)),
            Some(last_match(&FAILED, line.as_str()).unwrap_or(0)),
            Some(last_match(&SKIPPED, line.as_str()).unwrap_or(0)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};
    use crate::testrun::{classify, fixture_support, RunOutcome};
    use std::fs;

    fn words(command: &str) -> Vec<String> {
        command.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn command_uses_the_available_package_launcher() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        assert_eq!(
            ADAPTER.single_test_command("src/a.test.js", "suite passes", dir.path()),
            "npx jest src/a.test.js -t 'suite passes'"
        );
        fs::write(dir.path().join("bun.lock"), "").expect("bun lockfile");
        assert_eq!(
            ADAPTER.single_test_command("src/a.test.js", "suite passes", dir.path()),
            "bunx jest src/a.test.js -t 'suite passes'"
        );
        fs::remove_file(dir.path().join("bun.lock")).expect("remove bun lockfile");
        fs::write(dir.path().join("bun.lockb"), "").expect("binary bun lockfile");
        assert_eq!(
            ADAPTER.single_test_command("src/a.test.js", "suite passes", dir.path()),
            "bunx jest src/a.test.js -t 'suite passes'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let command = ADAPTER.single_test_command("a.test.js", HOSTILE_NAME, dir.path());
        assert_one_word(&command, &regex_literal(HOSTILE_NAME));
    }

    #[test]
    fn identifies_jest_with_supported_prefixes() {
        for command in [
            "jest",
            "bunx jest src/a.test.js",
            "npx jest --runInBand",
            "pnpm exec jest",
            "yarn jest",
        ] {
            assert!(ADAPTER.recognizes(&words(command)), "{command}");
        }
        for command in ["echo jest", "npx vitest", "jestish", "npm test"] {
            assert!(!ADAPTER.recognizes(&words(command)), "{command}");
        }
    }

    #[test]
    fn distinguishes_suite_runs_from_file_and_name_filters() {
        for command in [
            "jest",
            "npx jest --runInBand",
            "bunx jest --config jest.config.js",
        ] {
            assert!(ADAPTER.is_full_run(&words(command)), "{command}");
        }
        for command in [
            "jest src/a.test.js",
            "bunx jest -t alpha",
            "npx jest --testNamePattern=alpha",
            "jest --testPathPattern src",
            "jest --testPathPatterns=src",
            "npx vitest",
        ] {
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn selection_lists_each_file_once() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let targets = [
            TestTarget {
                file: "a.test.js".into(),
                name: Some("alpha".into()),
            },
            TestTarget {
                file: "b.test.js".into(),
                name: None,
            },
            TestTarget {
                file: "a.test.js".into(),
                name: Some("beta".into()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("npx jest a.test.js b.test.js")
        );
        assert_eq!(ADAPTER.select_command(&[], dir.path()), None);
        fs::write(dir.path().join("bun.lock"), "").expect("bun lockfile");
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("bunx jest a.test.js b.test.js")
        );
    }

    #[test]
    fn recorded_jest_runs_have_expected_outcomes() {
        let scenarios = fixture_support::scenarios("jest");
        for required in ["one-pass", "one-fail", "no-match", "suite"] {
            assert!(scenarios.iter().any(|scenario| scenario == required));
        }
        for scenario in scenarios {
            let (stdout, stderr, exit_code) = fixture_support::load("jest", &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let expected = match scenario.split('.').next() {
                Some("one-pass") => RunOutcome::Passed,
                Some("one-fail" | "suite") => RunOutcome::Failed,
                Some("no-match") => RunOutcome::NotSelected,
                other => panic!("unexpected Jest fixture: {other:?}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
