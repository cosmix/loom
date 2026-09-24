//! `ctest`: runs named CMake tests and reads the final suite summary.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{regex_literal, shell_quote};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Ctest;

pub static ADAPTER: Ctest = Ctest;

const COMMAND: &[&str] = &["ctest"];

/// CTest options that select a subset of tests or rerun previous failures.
const FILTER_FLAGS: &[&str] = &[
    "-R",
    "--tests-regex",
    "-E",
    "--exclude-regex",
    "-L",
    "--label-regex",
    "-LE",
    "-I",
    "--tests-information",
    "--rerun-failed",
];

/// Short CTest options also accept their value in the same word (`-Rname`).
const JOINED_FILTER_FLAGS: &[&str] = &["-R", "-E", "-L", "-LE", "-I"];

static FAILED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^[ \t]*\d+% tests passed, (\d+) tests failed out of \d+[ \t]*\r?$")
});
static TOTAL: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^[ \t]*\d+% tests passed, \d+ tests failed out of (\d+)[ \t]*\r?$")
});
static NO_MATCH: LazyLock<Regex> = LazyLock::new(|| pattern(r"No tests were found!!!"));
static CTEST_STARTED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^Test project "));
static BUILD_ERROR: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?im)\berror:|^CMake Error|g?make(?:\[\d+\])?: \*\*\*|ninja: build stopped:")
});

impl TestRunnerAdapter for Ctest {
    fn name(&self) -> &'static str {
        "ctest"
    }

    fn language(&self) -> &'static str {
        "cpp"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A filtered invocation cannot establish the status of the full suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let separate_or_equals = FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag));
        let joined_short = args.iter().any(|arg| {
            JOINED_FILTER_FLAGS.iter().any(|flag| {
                arg.strip_prefix(flag)
                    .is_some_and(|value| !value.is_empty())
            })
        });
        !separate_or_equals && !joined_short
    }

    fn single_test_command(&self, _file: &str, test: &str, _package_dir: &Path) -> String {
        let name = shell_quote(&format!("^{}$", regex_literal(test)));
        format!("cmake --build build && ctest --test-dir build -R {name} --output-on-failure")
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// The summary includes every completed test. A failed build has no CTest
    /// output because the `&&` pipeline stops at `cmake --build`.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if let (Some(failed), Some(total)) = (last_match(&FAILED, &text), last_match(&TOTAL, &text))
        {
            return summary_from_counts(Some(total.saturating_sub(failed)), Some(failed), None);
        }
        if NO_MATCH.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        RunSummary {
            build_failed: !CTEST_STARTED.is_match(&text) && BUILD_ERROR.is_match(&text),
            ..RunSummary::default()
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
    fn named_cmake_test_builds_before_ctest() {
        let command =
            ADAPTER.single_test_command("tests/alpha.cpp", "alpha_passes", Path::new("."));
        assert_eq!(
            command,
            "cmake --build build && ctest --test-dir build -R '^alpha_passes$' --output-on-failure"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("a.cpp", HOSTILE_NAME, Path::new("."));
        let expected = format!("^{}$", regex_literal(HOSTILE_NAME));
        assert_one_word(&command, &expected);
    }

    #[test]
    fn ctest_command_is_recognized_through_environment_prefixes() {
        assert!(ADAPTER.recognizes(&words("ctest --test-dir build")));
        assert!(ADAPTER.recognizes(&words("env CI=1 ctest --test-dir build")));
        assert!(ADAPTER.recognizes(&words("CI=1 /usr/bin/ctest --test-dir build")));
        assert!(!ADAPTER.recognizes(&words("cmake --build build")));
    }

    #[test]
    fn ctest_filters_exclude_full_suite_status() {
        assert!(ADAPTER.is_full_run(&words("ctest --test-dir build --output-on-failure")));
        assert!(!ADAPTER.is_full_run(&words("cmake --build build")));
        for flag in FILTER_FLAGS {
            assert!(
                !ADAPTER.is_full_run(&words(&format!("ctest {flag} alpha"))),
                "{flag}"
            );
            assert!(
                !ADAPTER.is_full_run(&words(&format!("ctest {flag}=alpha"))),
                "{flag}"
            );
        }
        for flag in JOINED_FILTER_FLAGS {
            assert!(
                !ADAPTER.is_full_run(&words(&format!("ctest {flag}alpha"))),
                "{flag}"
            );
        }
    }

    #[test]
    fn ctest_does_not_build_a_selection_command() {
        let targets = [TestTarget {
            file: "tests/alpha.cpp".to_string(),
            name: Some("alpha_passes".to_string()),
        }];
        assert_eq!(ADAPTER.select_command(&targets, Path::new(".")), None);
    }

    #[test]
    fn recorded_ctest_scenarios_have_expected_outcomes() {
        let recorded = scenarios(ADAPTER.name());
        assert!(!recorded.is_empty(), "ctest fixtures are missing");
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
                _ => panic!("unknown ctest fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
