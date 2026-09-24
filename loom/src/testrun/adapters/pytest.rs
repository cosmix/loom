//! `pytest` test-runner adapter. Invocation and output handling use the shared
//! helpers in [`crate::testrun::recognize`].

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{distinct_files, marker_choice, shell_quote, shell_words};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Pytest;

pub static ADAPTER: Pytest = Pytest;

const COMMAND: &[&str] = &["pytest"];

/// pytest options whose next word is a value, rather than a test path.
const VALUE_FLAGS: &[&str] = &[
    "-k",
    "-m",
    "--keyword",
    "--markexpr",
    "-c",
    "-o",
    "-p",
    "--config-file",
    "--override-ini",
    "--deselect",
    "--ignore",
    "--ignore-glob",
    "--confcutdir",
    "--rootdir",
    "--basetemp",
    "--capture",
    "--color",
    "--tb",
    "--durations",
    "--junitxml",
    "--junit-prefix",
    "--log-file",
    "--log-file-level",
    "--log-file-format",
    "--log-file-date-format",
    "--maxfail",
];

/// Counts are read only from pytest's final timing summary, so assertion text
/// and per-test failure details cannot be mistaken for completed tests.
static PASSED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[^\n]*\b(\d+) passed\b[^\n]*\bin \d+(?:\.\d+)?s\b"));
static FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[^\n]*\b(\d+) failed\b[^\n]*\bin \d+(?:\.\d+)?s\b"));
static SKIPPED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[^\n]*\b(\d+) skipped\b[^\n]*\bin \d+(?:\.\d+)?s\b"));
static NO_TESTS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^(?:=+\s*)?no tests ran\b"));
static ONLY_DESELECTED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^(?:=+\s*)?\d+ deselected in \d+(?:\.\d+)?s\b"));
static NOT_FOUND: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^ERROR: not found:"));
static COLLECTION_ERROR: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^_+\s+ERROR collecting\b|^ERROR collecting\b|^=+\s+ERRORS\s+=+|^ImportError while importing test module\b",
    )
});

fn runner_command(package_dir: &Path) -> &'static str {
    marker_choice(
        package_dir,
        &["uv.lock"],
        "uv run pytest",
        "python3 -m pytest",
    )
}

impl TestRunnerAdapter for Pytest {
    fn name(&self) -> &'static str {
        "pytest"
    }

    fn language(&self) -> &'static str {
        "python"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// File/node operands and keyword or marker filters select a subset.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let filter = [
            "-k",
            "-m",
            "--keyword",
            "--markexpr",
            "--deselect",
            "--ignore",
            "--ignore-glob",
        ]
        .iter()
        .any(|flag| has_flag(&args, flag));
        let compact_filter = args
            .iter()
            .any(|word| word.len() > 2 && (word.starts_with("-k") || word.starts_with("-m")));
        !filter && !compact_filter && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String {
        let node_id = shell_quote(&format!("{file}::{test}"));
        format!("{} {node_id} -q", runner_command(package_dir))
    }

    /// One pytest invocation names each file once; pytest runs every test in
    /// those files even when a target also supplies a test name.
    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        if files.is_empty() {
            return None;
        }
        let files = shell_words(files);
        Some(format!("{} {files} -q", runner_command(package_dir)))
    }

    /// Collection errors precede execution. A missing node id and an empty
    /// `-k` selection both report zero executed tests, despite different text.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let mut summary = summary_from_counts(
            last_match(&PASSED, &text),
            last_match(&FAILED, &text),
            last_match(&SKIPPED, &text),
        );
        if summary.executed.is_none()
            && (NO_TESTS.is_match(&text)
                || ONLY_DESELECTED.is_match(&text)
                || NOT_FOUND.is_match(&text))
        {
            summary.executed = Some(0);
        }
        summary.build_failed = COLLECTION_ERROR.is_match(&text);
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};
    use crate::testrun::{classify, fixture_support, RunOutcome};

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn builds_node_command_with_and_without_uv() {
        let dir = tempfile::tempdir().unwrap();
        let base = ADAPTER.single_test_command("tests/test_api.py", "TestApi::test_ok", dir.path());
        assert_eq!(
            base,
            "python3 -m pytest 'tests/test_api.py::TestApi::test_ok' -q"
        );
        std::fs::write(dir.path().join("uv.lock"), "").unwrap();
        let uv = ADAPTER.single_test_command("tests/test_api.py", "test_ok", dir.path());
        assert_eq!(uv, "uv run pytest 'tests/test_api.py::test_ok' -q");
    }

    #[test]
    fn hostile_test_name_stays_one_node_id() {
        let dir = tempfile::tempdir().unwrap();
        let command = ADAPTER.single_test_command("t.py", HOSTILE_NAME, dir.path());
        assert_one_word(&command, &format!("t.py::{HOSTILE_NAME}"));
    }

    #[test]
    fn recognizes_pytest_through_python_and_uv() {
        for words in [
            &["pytest", "-q"][..],
            &["python3", "-m", "pytest", "-q"][..],
            &["python", "-m", "pytest", "-q"][..],
            &["uv", "run", "pytest", "-q"][..],
            &["uv", "run", "python", "-m", "pytest", "-q"][..],
        ] {
            assert!(ADAPTER.recognizes(&argv(words)), "{words:?}");
        }
        for words in [
            &["python3", "-m", "unittest"][..],
            &["uv", "run", "ruff"][..],
        ] {
            assert!(!ADAPTER.recognizes(&argv(words)), "{words:?}");
        }
    }

    #[test]
    fn full_suite_requires_no_file_or_expression() {
        assert!(ADAPTER.is_full_run(&argv(&["python3", "-m", "pytest", "-q"])));
        assert!(ADAPTER.is_full_run(&argv(&["uv", "run", "pytest", "--color=no"])));
        for words in [
            &["pytest", "tests/test_api.py::test_ok"][..],
            &["pytest", "tests/test_api.py"][..],
            &["pytest", "-k", "test_ok"][..],
            &["pytest", "-m", "slow"][..],
            &["pytest", "-ktest_ok"][..],
            &["pytest", "--keyword=test_ok"][..],
        ] {
            assert!(!ADAPTER.is_full_run(&argv(words)), "{words:?}");
        }
    }

    #[test]
    fn selects_distinct_files_in_one_command() {
        let dir = tempfile::tempdir().unwrap();
        let targets = [
            TestTarget {
                file: "a.py".into(),
                name: Some("test_a".into()),
            },
            TestTarget {
                file: "b.py".into(),
                name: None,
            },
            TestTarget {
                file: "a.py".into(),
                name: Some("test_b".into()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("python3 -m pytest a.py b.py -q")
        );
        assert_eq!(ADAPTER.select_command(&[], dir.path()), None);
        std::fs::write(dir.path().join("uv.lock"), "").unwrap();
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("uv run pytest a.py b.py -q")
        );
    }

    #[test]
    fn fixture_scenarios_have_expected_outcomes_and_counts() {
        let scenarios = fixture_support::scenarios("pytest");
        assert!(!scenarios.is_empty());
        for scenario in scenarios {
            let (stdout, stderr, exit_code) = fixture_support::load("pytest", &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let expected = match scenario.split('.').next() {
                Some("one-pass") => RunOutcome::Passed,
                Some("one-fail" | "suite") => RunOutcome::Failed,
                Some("no-match") => RunOutcome::NotSelected,
                other => panic!("unexpected pytest fixture scenario: {other:?}"),
            };
            assert_eq!(
                classify(&summary, exit_code),
                expected,
                "{scenario}: {summary:?}"
            );
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }

    #[test]
    fn collection_error_is_a_build_failure() {
        let output = RunOutput {
            stdout: "================ ERRORS ================\nERROR collecting test_bad.py\n",
            stderr: "",
            exit_code: Some(2),
        };
        let summary = ADAPTER.parse(&output);
        assert!(summary.build_failed);
        assert_eq!(
            classify(&summary, output.exit_code),
            RunOutcome::BuildFailed
        );
    }
}
