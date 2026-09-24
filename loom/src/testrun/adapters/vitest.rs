//! `vitest` test-runner adapter. Command and output handling use the shared
//! recognition helpers; the final `Tests` line supplies the run counts.

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

pub struct Vitest;

pub static ADAPTER: Vitest = Vitest;

const COMMAND: &[&str] = &["vitest"];

/// Options whose separate values are not test-file positionals.
const VALUE_FLAGS: &[&str] = &[
    "-t",
    "--testNamePattern",
    "-c",
    "--config",
    "-r",
    "--root",
    "--pool",
    "--reporter",
    "--outputFile",
    "--environment",
    "--maxWorkers",
    "--minWorkers",
    "--testTimeout",
    "--hookTimeout",
];

/// The final aggregate line, separate from `Test Files` and file-level counts.
static TESTS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[ \t]*Tests[ \t]+([^\r\n]+)$"));
static PASSED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(\d+)[ \t]+passed\b"));
static FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(\d+)[ \t]+failed\b"));
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(\d+)[ \t]+skipped\b"));

impl TestRunnerAdapter for Vitest {
    fn name(&self) -> &'static str {
        "vitest"
    }

    fn language(&self) -> &'static str {
        "javascript"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// `run` and `watch` choose execution mode; remaining positionals select
    /// test files, while `-t` and `--testNamePattern` select test names.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let rest = match args.first().map(String::as_str) {
            Some("run" | "watch") => &args[1..],
            _ => &args[..],
        };
        !has_flag(rest, "-t")
            && !has_flag(rest, "--testNamePattern")
            && positionals(rest, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String {
        let runner = package_runner(package_dir);
        let file = shell_word(file);
        let name = shell_quote(&regex_literal(test));
        format!("{runner} vitest run {file} -t {name}")
    }

    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        if files.is_empty() {
            return None;
        }
        let runner = package_runner(package_dir);
        Some(format!("{runner} vitest run {}", shell_words(files)))
    }

    /// Counts only the aggregate `Tests` line. Vitest reports an unmatched
    /// name as all skipped and may exit successfully, so that means zero ran.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let Some(line) = TESTS
            .captures_iter(&text)
            .last()
            .and_then(|captures| captures.get(1))
        else {
            return RunSummary::default();
        };
        let line = line.as_str();
        let mut summary = summary_from_counts(
            last_match(&PASSED, line),
            last_match(&FAILED, line),
            last_match(&SKIPPED, line),
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
    use crate::testrun::fixture_support::{load, scenarios};
    use crate::testrun::{classify, RunOutcome};

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    fn target(file: &str, name: Option<&str>) -> TestTarget {
        TestTarget {
            file: file.to_string(),
            name: name.map(str::to_string),
        }
    }

    #[test]
    fn single_test_command_uses_the_available_package_runner() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let file = "src/math.test.js";
        let test = "math adds values";
        assert_eq!(
            ADAPTER.single_test_command(file, test, dir.path()),
            "npx vitest run src/math.test.js -t 'math adds values'"
        );

        std::fs::write(dir.path().join("bun.lock"), "").expect("bun lockfile");
        assert_eq!(
            ADAPTER.single_test_command(file, test, dir.path()),
            "bunx vitest run src/math.test.js -t 'math adds values'"
        );
        std::fs::remove_file(dir.path().join("bun.lock")).expect("remove bun lockfile");
        std::fs::write(dir.path().join("bun.lockb"), "").expect("legacy bun lockfile");
        assert_eq!(
            ADAPTER.single_test_command(file, test, dir.path()),
            "bunx vitest run src/math.test.js -t 'math adds values'"
        );
    }

    #[test]
    fn recognizes_direct_and_wrapped_vitest_commands() {
        for command in [
            words(&["vitest"]),
            words(&["vitest", "run"]),
            words(&["vitest", "watch"]),
            words(&["bunx", "vitest", "run"]),
            words(&["npx", "vitest", "run"]),
            words(&["pnpm", "exec", "vitest", "run"]),
            words(&["yarn", "vitest", "run"]),
        ] {
            assert!(ADAPTER.recognizes(&command), "{command:?}");
        }
        for command in [words(&["jest", "run"]), words(&["echo", "vitest", "run"])] {
            assert!(!ADAPTER.recognizes(&command), "{command:?}");
        }
    }

    #[test]
    fn full_run_requires_no_file_or_name_filter() {
        assert!(ADAPTER.is_full_run(&words(&["vitest", "run"])));
        assert!(ADAPTER.is_full_run(&words(&[
            "npx",
            "vitest",
            "watch",
            "--config",
            "vitest.config.js",
        ])));
        assert!(!ADAPTER.is_full_run(&words(&["vitest", "run", "src/math.test.js"])));
        assert!(!ADAPTER.is_full_run(&words(&["vitest", "run", "-t", "adds values"])));
        assert!(!ADAPTER.is_full_run(&words(&["vitest", "--testNamePattern=adds"])));
        assert!(!ADAPTER.is_full_run(&words(&["echo", "vitest", "run"])));
    }

    #[test]
    fn selection_lists_all_target_files_in_one_command() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let targets = [
            target("src/a.test.js", Some("alpha")),
            target("src/b.test.js", None),
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("npx vitest run src/a.test.js src/b.test.js")
        );
        assert_eq!(ADAPTER.select_command(&[], dir.path()), None);
    }

    #[test]
    fn selection_names_a_file_with_two_targets_once() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let targets = [
            target("src/a.test.js", Some("alpha")),
            target("src/a.test.js", Some("beta")),
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("npx vitest run src/a.test.js")
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let command = ADAPTER.single_test_command("a.test.js", HOSTILE_NAME, dir.path());
        assert_one_word(&command, &regex_literal(HOSTILE_NAME));
    }

    fn fixture_expectation(scenario: &str) -> (RunSummary, RunOutcome) {
        let base = scenario.split('.').next().unwrap_or(scenario);
        match base {
            "one-pass" => (
                RunSummary {
                    executed: Some(1),
                    passed: Some(1),
                    skipped: Some(2),
                    ..RunSummary::default()
                },
                RunOutcome::Passed,
            ),
            "one-fail" => (
                RunSummary {
                    executed: Some(1),
                    failed: Some(1),
                    skipped: Some(2),
                    ..RunSummary::default()
                },
                RunOutcome::Failed,
            ),
            "no-match" => (
                RunSummary {
                    executed: Some(0),
                    skipped: Some(3),
                    ..RunSummary::default()
                },
                RunOutcome::NotSelected,
            ),
            "suite" => (
                RunSummary {
                    executed: Some(3),
                    passed: Some(2),
                    failed: Some(1),
                    ..RunSummary::default()
                },
                RunOutcome::Failed,
            ),
            _ => panic!("unexpected vitest fixture: {scenario}"),
        }
    }

    #[test]
    fn parses_and_classifies_each_recorded_scenario() {
        let recordings = scenarios("vitest");
        assert_eq!(recordings.len(), 8);
        for scenario in recordings {
            let (stdout, stderr, exit_code) = load("vitest", &scenario);
            let output = RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            };
            let summary = ADAPTER.parse(&output);
            let (expected_summary, expected_outcome) = fixture_expectation(&scenario);
            assert_eq!(summary, expected_summary, "{scenario}");
            assert_eq!(
                classify(&summary, exit_code),
                expected_outcome,
                "{scenario}"
            );
        }
    }
}
