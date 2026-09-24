//! `phpunit`: recognises PHPUnit commands and reads their final test summary.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{regex_literal, shell_quote, shell_word};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Phpunit;

pub static ADAPTER: Phpunit = Phpunit;

const COMMAND: &[&str] = &["phpunit"];

/// PHPUnit options whose values are separate words rather than positionals.
const VALUE_FLAGS: &[&str] = &[
    "--configuration",
    "-c",
    "--bootstrap",
    "--cache-directory",
    "--coverage-clover",
    "--coverage-cobertura",
    "--coverage-crap4j",
    "--coverage-filter",
    "--coverage-html",
    "--coverage-php",
    "--coverage-text",
    "--coverage-xml",
    "--exclude-group",
    "--filter",
    "--group",
    "--log-junit",
    "--order-by",
    "--random-order-seed",
    "--testdox-html",
    "--testdox-text",
    "--testsuite",
];

/// Filters that make the invocation narrower than the whole suite.
const FILTER_FLAGS: &[&str] = &["--filter", "--group", "--exclude-group", "--testsuite"];

static OK_TESTS: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^OK \((\d+) tests?, \d+ assertions?\)[ \t]*\r?$"));
static TOTAL: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^Tests:[ \t]*(\d+)\b"));
static FAILURES: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^Tests:[^\r\n]*\bFailures:[ \t]*(\d+)\b"));
static ERRORS: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^Tests:[^\r\n]*\bErrors:[ \t]*(\d+)\b"));
static SKIPPED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^Tests:[^\r\n]*\bSkipped:[ \t]*(\d+)\b"));
static INCOMPLETE: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^Tests:[^\r\n]*\bIncomplete:[ \t]*(\d+)\b"));
static NO_TESTS: LazyLock<Regex> = LazyLock::new(|| pattern(r"No tests executed!"));

/// Arguments after `phpunit`, including its `php vendor/bin/phpunit` form.
fn phpunit_args(argv: &[String]) -> Option<Vec<String>> {
    command_args(argv, COMMAND).or_else(|| {
        let php_args = command_args(argv, &["php"])?;
        command_args(&php_args, COMMAND)
    })
}

impl TestRunnerAdapter for Phpunit {
    fn name(&self) -> &'static str {
        "phpunit"
    }

    fn language(&self) -> &'static str {
        "php"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        phpunit_args(argv).is_some()
    }

    /// A filter or file/directory positional excludes part of the suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = phpunit_args(argv) else {
            return false;
        };
        !FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag))
            && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let name = shell_quote(&regex_literal(test));
        format!("vendor/bin/phpunit --filter {name} {}", shell_word(file))
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// The final `Tests:` line takes precedence over `OK`; incomplete tests
    /// did not execute and share the skipped count in `RunSummary`.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if NO_TESTS.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        let Some(tests) = last_match(&TOTAL, &text).or_else(|| last_match(&OK_TESTS, &text)) else {
            return RunSummary::default();
        };
        let skipped = match (last_match(&SKIPPED, &text), last_match(&INCOMPLETE, &text)) {
            (None, None) => None,
            (skipped, incomplete) => {
                Some(skipped.unwrap_or(0).saturating_add(incomplete.unwrap_or(0)))
            }
        };
        let executed = tests.saturating_sub(skipped.unwrap_or(0));
        let failed = last_match(&FAILURES, &text)
            .unwrap_or(0)
            .saturating_add(last_match(&ERRORS, &text).unwrap_or(0));
        RunSummary {
            executed: Some(executed),
            passed: Some(executed.saturating_sub(failed)),
            failed: Some(failed),
            skipped,
            build_failed: false,
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
    fn method_filter_uses_the_declaring_file() {
        let command =
            ADAPTER.single_test_command("tests/ExampleTest.php", "testAlphaPasses", Path::new("."));
        assert_eq!(
            command,
            "vendor/bin/phpunit --filter 'testAlphaPasses' tests/ExampleTest.php"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("ATest.php", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &regex_literal(HOSTILE_NAME));
    }

    #[test]
    fn phpunit_executable_and_php_script_are_recognized() {
        assert!(ADAPTER.recognizes(&words("vendor/bin/phpunit")));
        assert!(ADAPTER.recognizes(&words("php vendor/bin/phpunit")));
        assert!(ADAPTER.recognizes(&words("phpunit")));
        assert!(!ADAPTER.recognizes(&words("vendor/bin/pest")));
    }

    #[test]
    fn phpunit_filters_and_paths_narrow_the_run() {
        assert!(ADAPTER.is_full_run(&words("vendor/bin/phpunit")));
        assert!(ADAPTER.is_full_run(&words("phpunit --configuration phpunit.xml")));
        for flag in FILTER_FLAGS {
            assert!(!ADAPTER.is_full_run(&words(&format!("phpunit {flag} alpha"))));
            assert!(!ADAPTER.is_full_run(&words(&format!("phpunit {flag}=alpha"))));
        }
        assert!(!ADAPTER.is_full_run(&words("phpunit tests/ExampleTest.php")));
        assert!(!ADAPTER.is_full_run(&words("phpunit tests")));
    }

    #[test]
    fn phpunit_has_no_selection_command() {
        let targets = [TestTarget {
            file: "tests/ExampleTest.php".to_string(),
            name: Some("testAlphaPasses".to_string()),
        }];
        assert_eq!(ADAPTER.select_command(&targets, Path::new(".")), None);
    }

    #[test]
    fn phpunit_fixture_outcomes_match_the_scenarios() {
        let recorded = scenarios(ADAPTER.name());
        assert!(!recorded.is_empty(), "phpunit fixtures are missing");
        for scenario in recorded {
            let (stdout, stderr, exit_code) = load(ADAPTER.name(), &scenario);
            let output = RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            };
            let summary = ADAPTER.parse(&output);
            let expected = match scenario.as_str() {
                "one-pass" => RunOutcome::Passed,
                "one-fail" | "suite" => RunOutcome::Failed,
                "no-match" => RunOutcome::NotSelected,
                _ => panic!("unknown phpunit fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }

    #[test]
    fn phpunit_summary_combines_errors_failures_and_nonexecuted_tests() {
        let output = RunOutput {
            stdout: "FAILURES!\nTests: 5, Assertions: 3, Errors: 1, Failures: 1, Skipped: 1, Incomplete: 1.\n",
            stderr: "",
            exit_code: Some(1),
        };
        assert_eq!(
            ADAPTER.parse(&output),
            RunSummary {
                executed: Some(3),
                passed: Some(1),
                failed: Some(2),
                skipped: Some(2),
                build_failed: false,
            }
        );
    }
}
