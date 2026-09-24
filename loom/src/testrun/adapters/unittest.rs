//! Python's `unittest` runner. The runner writes its summary to stderr and
//! reports a missing named test as a synthetic `_FailedTest` loader error.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{marker_choice, shell_word, shell_words};
use crate::testrun::recognize::{
    combined_output, command_args, count_matches, has_flag, last_match, pattern, positionals,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Unittest;

pub static ADAPTER: Unittest = Unittest;

const COMMAND: &[&str] = &["unittest"];

/// Options that consume a word without selecting a test themselves.
const VALUE_FLAGS: &[&str] = &[
    "--durations",
    "-k",
    "-s",
    "--start-directory",
    "-p",
    "--pattern",
    "-t",
    "--top-level-directory",
];

/// Options that restrict which tests `discover` or `unittest` runs.
const FILTER_FLAGS: &[&str] = &[
    "-k",
    "-s",
    "--start-directory",
    "-p",
    "--pattern",
    "-t",
    "--top-level-directory",
];

static RAN: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^Ran (\d+) tests?\b"));
static FAILURES: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^FAILED \([^\n)]*\bfailures=(\d+)"));
static ERRORS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^FAILED \([^\n)]*\berrors=(\d+)"));
static SKIPPED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^(?:FAILED|OK) \([^\n)]*\bskipped=(\d+)"));
static LOADER_ERROR: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^ERROR: [^\n]*\(unittest\.loader\._FailedTest\.[^)\n]+\)"));

fn command_prefix(package_dir: &Path) -> &'static str {
    marker_choice(
        package_dir,
        &["uv.lock"],
        "uv run python -m unittest",
        "python3 -m unittest",
    )
}

impl TestRunnerAdapter for Unittest {
    fn name(&self) -> &'static str {
        "unittest"
    }

    fn language(&self) -> &'static str {
        "python"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A bare invocation or unqualified `discover` runs the whole suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        if FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag)) {
            return false;
        }
        let operands = positionals(&args, VALUE_FLAGS);
        operands.is_empty() || operands == ["discover"]
    }

    fn single_test_command(&self, _file: &str, test: &str, package_dir: &Path) -> String {
        format!("{} {} -v", command_prefix(package_dir), shell_word(test))
    }

    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String> {
        let selectors: Vec<&str> = targets
            .iter()
            .map(|target| target.name.as_deref().unwrap_or(&target.file))
            .filter(|selector| !selector.is_empty())
            .collect();
        if selectors.is_empty() {
            return None;
        }
        let selectors = shell_words(selectors);
        Some(format!("{} {selectors} -v", command_prefix(package_dir)))
    }

    /// `_FailedTest` is a loader placeholder, not a test that ran. Remove its
    /// reported error and run count before deriving the actual test counts.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let loader_errors = count_matches(&LOADER_ERROR, &text);
        let skipped = last_match(&SKIPPED, &text);
        let executed = last_match(&RAN, &text)
            .map(|ran| {
                ran.saturating_sub(skipped.unwrap_or(0))
                    .saturating_sub(loader_errors)
            })
            .or_else(|| (loader_errors > 0).then_some(0));
        let reported_failures = last_match(&FAILURES, &text)
            .unwrap_or(0)
            .saturating_add(last_match(&ERRORS, &text).unwrap_or(0));
        let failed = executed.map(|_| reported_failures.saturating_sub(loader_errors));
        RunSummary {
            executed,
            passed: executed.map(|total| total.saturating_sub(failed.unwrap_or(0))),
            failed,
            skipped,
            build_failed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};
    use crate::testrun::fixture_support;
    use crate::testrun::{classify, RunOutcome};

    fn words(command: &str) -> Vec<String> {
        command.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn commands_use_python_or_uv_for_dotted_test_names() {
        let dir = tempfile::tempdir().expect("temporary package");
        let test = "test_fixture.FixtureTests.test_alpha_passes";
        assert_eq!(
            ADAPTER.single_test_command("test_fixture.py", test, dir.path()),
            format!("python3 -m unittest {test} -v")
        );
        std::fs::write(dir.path().join("uv.lock"), "").expect("uv lockfile");
        assert_eq!(
            ADAPTER.single_test_command("test_fixture.py", test, dir.path()),
            format!("uv run python -m unittest {test} -v")
        );
    }

    #[test]
    fn hostile_test_name_stays_one_word() {
        let dir = tempfile::tempdir().expect("temporary package");
        let command = ADAPTER.single_test_command("t.py", HOSTILE_NAME, dir.path());
        assert_one_word(&command, HOSTILE_NAME);
    }

    #[test]
    fn invocation_matching_accepts_wrappers() {
        assert!(ADAPTER.recognizes(&words("python3 -m unittest discover")));
        assert!(ADAPTER.recognizes(&words("uv run python -m unittest -v")));
        assert!(!ADAPTER.recognizes(&words("python3 -m pytest -q")));
    }

    #[test]
    fn suite_detection_rejects_test_and_discovery_filters() {
        assert!(ADAPTER.is_full_run(&words("python3 -m unittest")));
        assert!(ADAPTER.is_full_run(&words("unittest discover -v")));
        assert!(!ADAPTER.is_full_run(&words(
            "python3 -m unittest test_fixture.FixtureTests.test_alpha_passes -v"
        )));
        assert!(!ADAPTER.is_full_run(&words("unittest discover -p test_one.py")));
    }

    #[test]
    fn selection_includes_named_and_file_targets() {
        let dir = tempfile::tempdir().expect("temporary package");
        let targets = [
            TestTarget {
                file: "test_fixture.py".to_string(),
                name: Some("test_fixture.FixtureTests.test_alpha_passes".to_string()),
            },
            TestTarget {
                file: "test_other.py".to_string(),
                name: None,
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some(
                "python3 -m unittest test_fixture.FixtureTests.test_alpha_passes test_other.py -v"
            )
        );
        assert_eq!(ADAPTER.select_command(&[], dir.path()), None);
    }

    #[test]
    fn captured_runs_have_the_expected_verdicts() {
        let scenarios = fixture_support::scenarios("unittest");
        assert!(!scenarios.is_empty());
        for scenario in scenarios {
            let (stdout, stderr, exit_code) = fixture_support::load("unittest", &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let base = scenario.split('.').next().expect("scenario name");
            let expected = match base {
                "one-pass" => RunOutcome::Passed,
                "one-fail" | "suite" => RunOutcome::Failed,
                "no-match" => RunOutcome::NotSelected,
                "build-error" => RunOutcome::BuildFailed,
                other => panic!("unknown scenario: {other}"),
            };
            assert_eq!(
                classify(&summary, exit_code),
                expected,
                "{scenario}: {summary:?}"
            );
            if base == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
            if base == "no-match" {
                assert_eq!(summary.executed, Some(0));
            }
        }
    }
}
