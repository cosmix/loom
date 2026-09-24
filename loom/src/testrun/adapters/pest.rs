//! Pest runs PHP tests and reports the completed counts on its `Tests:` line.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{regex_literal, shell_quote, shell_word};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Pest;

pub static ADAPTER: Pest = Pest;

const COMMAND: &[&str] = &["pest"];
const PHP_COMMAND: &[&str] = &["php"];

/// Options whose following word is a value, rather than a test file or directory.
const VALUE_FLAGS: &[&str] = &[
    "--filter",
    "--group",
    "--exclude-group",
    "--testsuite",
    "--configuration",
    "-c",
    "--bootstrap",
    "--colors",
    "--cache-directory",
    "--processes",
    "--order-by",
    "--random-order-seed",
    "--log-junit",
    "--coverage-html",
    "--coverage-clover",
    "--coverage-xml",
];

/// Pest options that select less than the whole suite.
const FILTER_FLAGS: &[&str] = &["--filter", "--group", "--exclude-group", "--testsuite"];

static TESTS_LINE: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[ \t]*Tests:[ \t]*([^\r\n]*)"));
static PASSED: LazyLock<Regex> = LazyLock::new(|| pattern(r"\b(\d+)[ \t]+passed\b"));
static FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"\b(\d+)[ \t]+failed\b"));
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| pattern(r"\b(\d+)[ \t]+skipped\b"));
static TODO: LazyLock<Regex> = LazyLock::new(|| pattern(r"\b(\d+)[ \t]+todo\b"));
static NO_MATCH: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\bNo tests found\b"));

/// The Pest arguments after either its executable or a `php pest` invocation.
fn pest_args(argv: &[String]) -> Option<Vec<String>> {
    command_args(argv, COMMAND).or_else(|| {
        let php_args = command_args(argv, PHP_COMMAND)?;
        command_args(&php_args, COMMAND)
    })
}

impl TestRunnerAdapter for Pest {
    fn name(&self) -> &'static str {
        "pest"
    }

    fn language(&self) -> &'static str {
        "php"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        pest_args(argv).is_some()
    }

    /// Any filter or positional path limits the run to part of the suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = pest_args(argv) else {
            return false;
        };
        !FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag))
            && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let name = shell_quote(&regex_literal(test));
        format!("vendor/bin/pest --filter {name} {}", shell_word(file))
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// Pest omits zero categories and may reorder the categories on `Tests:`.
    /// Todo tests have not run, so they join skipped tests in this summary.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let tests = TESTS_LINE
            .captures_iter(&text)
            .last()
            .and_then(|captures| captures.get(1).map(|value| value.as_str()));
        if let Some(tests) = tests {
            let passed = last_match(&PASSED, tests);
            let failed = last_match(&FAILED, tests);
            let skipped = last_match(&SKIPPED, tests);
            let todo = last_match(&TODO, tests);
            let mut summary = summary_from_counts(passed, failed, None);
            if summary.executed.is_none() && (skipped.is_some() || todo.is_some()) {
                summary.executed = Some(0);
            }
            summary.skipped = match (skipped, todo) {
                (None, None) => None,
                _ => Some(skipped.unwrap_or(0).saturating_add(todo.unwrap_or(0))),
            };
            return summary;
        }
        if NO_MATCH.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        RunSummary::default()
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
    fn named_pest_test_uses_description_filter_and_file() {
        let command = ADAPTER.single_test_command(
            "tests/Feature/ExampleTest.php",
            "alpha passes",
            Path::new("."),
        );
        assert_eq!(
            command,
            "vendor/bin/pest --filter 'alpha passes' tests/Feature/ExampleTest.php"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("ATest.php", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &regex_literal(HOSTILE_NAME));
    }

    #[test]
    fn pest_executable_and_php_invocation_are_recognized() {
        assert!(ADAPTER.recognizes(&words("pest")));
        assert!(ADAPTER.recognizes(&words("vendor/bin/pest --filter alpha")));
        assert!(ADAPTER.recognizes(&words("php vendor/bin/pest --filter alpha")));
        assert!(!ADAPTER.recognizes(&words("vendor/bin/phpunit --filter alpha")));
    }

    #[test]
    fn pest_filters_and_paths_limit_suite_scope() {
        assert!(ADAPTER.is_full_run(&words("vendor/bin/pest --colors=never")));
        assert!(ADAPTER.is_full_run(&words("php vendor/bin/pest --configuration phpunit.xml")));
        assert!(!ADAPTER.is_full_run(&words("vendor/bin/phpunit")));
        for flag in FILTER_FLAGS {
            assert!(
                !ADAPTER.is_full_run(&words(&format!("pest {flag} alpha"))),
                "{flag}"
            );
            assert!(
                !ADAPTER.is_full_run(&words(&format!("pest {flag}=alpha"))),
                "{flag}"
            );
        }
        assert!(!ADAPTER.is_full_run(&words("pest tests/Feature/ExampleTest.php")));
        assert!(!ADAPTER.is_full_run(&words("pest tests/Feature")));
    }

    #[test]
    fn pest_has_no_multi_target_selection_command() {
        let targets = [TestTarget {
            file: "tests/Feature/ExampleTest.php".to_string(),
            name: Some("alpha passes".to_string()),
        }];
        assert_eq!(ADAPTER.select_command(&targets, Path::new(".")), None);
    }

    #[test]
    fn pest_summary_reads_categories_in_any_order() {
        let output = RunOutput {
            stdout: "Tests:    2 skipped, 1 todo, 1 failed, 2 passed (3 assertions)\n",
            stderr: "",
            exit_code: Some(1),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!(
            (summary.executed, summary.passed, summary.failed),
            (Some(3), Some(2), Some(1))
        );
        assert_eq!(summary.skipped, Some(3));
    }

    #[test]
    fn pest_fixtures_parse_and_classify() {
        let recorded = scenarios(ADAPTER.name());
        assert!(!recorded.is_empty(), "pest fixtures are missing");
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
                _ => panic!("unknown pest fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
