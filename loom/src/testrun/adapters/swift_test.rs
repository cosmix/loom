//! `swift test`: runs SwiftPM tests and reads XCTest suite summaries.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{regex_literal, shell_quote};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct SwiftTest;

pub static ADAPTER: SwiftTest = SwiftTest;

const COMMAND: &[&str] = &["swift", "test"];

/// XCTest prints this line for each suite level, ending with the whole run.
static TOTAL: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[ \t]*Executed (\d+) tests?, with \d+ failures?\b"));
static FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[ \t]*Executed \d+ tests?, with (\d+) failures?\b"));
static SUITE_STARTED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^Test Suite .* started\b"));
static ERROR: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[^\r\n]*\berror:"));

impl TestRunnerAdapter for SwiftTest {
    fn name(&self) -> &'static str {
        "swift-test"
    }

    fn language(&self) -> &'static str {
        "swift"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// `--filter` and `--skip` both limit the test set, with either value form.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        !has_flag(&args, "--filter") && !has_flag(&args, "--skip")
    }

    fn single_test_command(&self, _file: &str, test: &str, _package_dir: &Path) -> String {
        format!("swift test --filter {}", shell_quote(&regex_literal(test)))
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// Take the last suite total; earlier lines count nested suites again.
    /// An `error:` before any suite starts is a compiler failure, while an
    /// assertion diagnostic after a suite starts belongs to a test failure.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let suite_start = SUITE_STARTED.find(&text).map(|found| found.start());
        let build_failed = ERROR
            .find(&text)
            .is_some_and(|found| suite_start.is_none_or(|start| found.start() < start));
        let mut summary = match (last_match(&TOTAL, &text), last_match(&FAILED, &text)) {
            (Some(total), Some(failed)) => {
                summary_from_counts(Some(total.saturating_sub(failed)), Some(failed), None)
            }
            _ => RunSummary::default(),
        };
        summary.build_failed = build_failed;
        summary
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
    fn swift_filter_uses_xctest_identifier() {
        let command = ADAPTER.single_test_command(
            "Tests/FooTests.swift",
            "ModuleTests.FooTests/testBar",
            Path::new("."),
        );
        assert_eq!(
            command,
            r"swift test --filter 'ModuleTests\.FooTests/testBar'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("A.swift", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &regex_literal(HOSTILE_NAME));
    }

    #[test]
    fn swift_test_command_recognition() {
        assert!(ADAPTER.recognizes(&words("swift test")));
        assert!(ADAPTER.recognizes(&words("env CI=1 swift test --parallel")));
        assert!(ADAPTER.recognizes(&words("CI=1 swift test")));
        assert!(!ADAPTER.recognizes(&words("swift build")));
    }

    #[test]
    fn swift_test_filter_limits_suite() {
        assert!(ADAPTER.is_full_run(&words("swift test --parallel")));
        assert!(!ADAPTER.is_full_run(&words("swift build")));
        for flag in ["--filter", "--skip"] {
            assert!(!ADAPTER.is_full_run(&words(&format!("swift test {flag} Foo"))));
            assert!(!ADAPTER.is_full_run(&words(&format!("swift test {flag}=Foo"))));
        }
    }

    #[test]
    fn swift_test_has_no_multi_target_command() {
        let targets = [TestTarget {
            file: "Tests/FooTests.swift".to_string(),
            name: Some("ModuleTests.FooTests/testBar".to_string()),
        }];
        assert_eq!(ADAPTER.select_command(&targets, Path::new(".")), None);
    }

    #[test]
    fn swift_fixture_outcomes_and_counts() {
        let names = scenarios(ADAPTER.name());
        assert_eq!(names.len(), 5);
        for scenario in names {
            let (stdout, stderr, exit_code) = load(ADAPTER.name(), &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let expected = match scenario.as_str() {
                "one-pass" => RunOutcome::Passed,
                "one-fail" | "suite" => RunOutcome::Failed,
                "no-match" => RunOutcome::NotSelected,
                "build-error" => RunOutcome::BuildFailed,
                other => panic!("unexpected scenario: {other}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
