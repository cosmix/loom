//! Maven Surefire: recognises lifecycle phases that run tests and reads the
//! final aggregate result, after any per-class result lines.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::shell_quote;
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Maven;

pub static ADAPTER: Maven = Maven;

/// Lifecycle phases from `test` onward that run Surefire tests.
const TEST_PHASES: &[&str] = &[
    "test",
    "prepare-package",
    "package",
    "pre-integration-test",
    "integration-test",
    "post-integration-test",
    "verify",
    "install",
    "deploy",
];

/// Maven options whose values are separate command words.
const VALUE_FLAGS: &[&str] = &[
    "-D",
    "--define",
    "-f",
    "--file",
    "-pl",
    "--projects",
    "-rf",
    "--resume-from",
    "-P",
    "--activate-profiles",
    "-s",
    "--settings",
    "-gs",
    "--global-settings",
    "-t",
    "--toolchains",
    "-l",
    "--log-file",
];

/// Options that build only some of a multi-module project's modules.
const MODULE_FLAGS: &[&str] = &["-pl", "--projects", "-rf", "--resume-from"];

static TOTAL: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:INFO|ERROR)\] Tests run: (\d+), Failures: \d+, Errors: \d+, Skipped: \d+\r?$",
    )
});
static FAILURES: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:INFO|ERROR)\] Tests run: \d+, Failures: (\d+), Errors: \d+, Skipped: \d+\r?$",
    )
});
static ERRORS: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:INFO|ERROR)\] Tests run: \d+, Failures: \d+, Errors: (\d+), Skipped: \d+\r?$",
    )
});
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^\[(?:INFO|ERROR)\] Tests run: \d+, Failures: \d+, Errors: \d+, Skipped: (\d+)\r?$",
    )
});
static NO_MATCH: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"No tests matching pattern|No tests were executed"));
static BUILD_ERROR: LazyLock<Regex> = LazyLock::new(|| pattern(r"COMPILATION ERROR"));

fn maven_args(argv: &[String]) -> Option<Vec<String>> {
    command_args(argv, &["mvn"]).or_else(|| command_args(argv, &["mvnw"]))
}

fn runs_tests(args: &[String]) -> bool {
    positionals(args, VALUE_FLAGS)
        .iter()
        .any(|phase| TEST_PHASES.contains(phase))
}

fn has_test_filter(args: &[String]) -> bool {
    has_flag(args, "-Dtest")
        || has_flag(args, "-Dit.test")
        || args.windows(2).any(|pair| {
            (pair[0] == "-D" || pair[0] == "--define")
                && (pair[1].starts_with("test=") || pair[1].starts_with("it.test="))
        })
}

impl TestRunnerAdapter for Maven {
    fn name(&self) -> &'static str {
        "maven"
    }

    fn language(&self) -> &'static str {
        "java"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        maven_args(argv).is_some_and(|args| runs_tests(&args))
    }

    /// A Surefire test filter or a module selection leaves tests out.
    fn is_full_run(&self, argv: &[String]) -> bool {
        maven_args(argv).is_some_and(|args| {
            let modules = MODULE_FLAGS.iter().any(|flag| has_flag(&args, flag));
            runs_tests(&args) && !has_test_filter(&args) && !modules
        })
    }

    fn single_test_command(&self, _file: &str, test: &str, _package_dir: &Path) -> String {
        format!("mvn -q test -Dtest={}", shell_quote(test))
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// Surefire prints one line per class and then an aggregate. The last line
    /// is the aggregate; `Tests run` includes skipped tests.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let total = last_match(&TOTAL, &text);
        let failures = last_match(&FAILURES, &text);
        let errors = last_match(&ERRORS, &text);
        let skipped = last_match(&SKIPPED, &text);
        if let (Some(total), Some(failures), Some(errors), Some(skipped)) =
            (total, failures, errors, skipped)
        {
            let failed = failures.saturating_add(errors);
            let executed = total.saturating_sub(skipped);
            return RunSummary {
                executed: Some(executed),
                passed: Some(executed.saturating_sub(failed)),
                failed: Some(failed),
                skipped: Some(skipped),
                build_failed: false,
            };
        }
        if NO_MATCH.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        RunSummary {
            build_failed: BUILD_ERROR.is_match(&text),
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
    fn named_maven_method_uses_surefire_filter() {
        let command = ADAPTER.single_test_command(
            "src/test/java/AlphaTest.java",
            "AlphaTest#passes",
            Path::new("."),
        );
        assert_eq!(command, "mvn -q test -Dtest='AlphaTest#passes'");
    }

    #[test]
    fn lifecycle_phases_run_maven_tests() {
        assert!(ADAPTER.recognizes(&words("mvn test")));
        assert!(ADAPTER.recognizes(&words("./mvnw -q verify")));
        assert!(ADAPTER.recognizes(&words("mvn prepare-package")));
        assert!(ADAPTER.recognizes(&words("mvn clean package")));
        assert!(ADAPTER.recognizes(&words("mvn install")));
        assert!(!ADAPTER.recognizes(&words("mvn compile")));
    }

    #[test]
    fn surefire_filters_exclude_full_suite_status() {
        assert!(ADAPTER.is_full_run(&words("mvn test")));
        assert!(ADAPTER.is_full_run(&words("./mvnw -q verify")));
        assert!(!ADAPTER.is_full_run(&words("mvn test -Dtest=AlphaTest")));
        assert!(!ADAPTER.is_full_run(&words("mvn test -D test=AlphaTest")));
        assert!(!ADAPTER.is_full_run(&words("mvn verify -Dit.test=AlphaIT")));
        assert!(!ADAPTER.is_full_run(&words("mvn verify -D it.test=AlphaIT")));
        assert!(!ADAPTER.is_full_run(&words("mvn compile")));
    }

    #[test]
    fn module_selection_excludes_full_suite_status() {
        for command in [
            "mvn test -pl core",
            "mvn test --projects=core,api",
            "mvn verify -rf :core",
            "mvn verify --resume-from :core",
        ] {
            assert!(ADAPTER.recognizes(&words(command)), "{command}");
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn hostile_test_name_stays_one_property_word() {
        let command = ADAPTER.single_test_command("A.java", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &format!("-Dtest={HOSTILE_NAME}"));
    }

    #[test]
    fn maven_has_no_selection_command() {
        let target = TestTarget {
            file: "src/test/java/AlphaTest.java".to_string(),
            name: Some("AlphaTest#passes".to_string()),
        };
        assert_eq!(ADAPTER.select_command(&[target], Path::new(".")), None);
    }

    #[test]
    fn documented_surefire_scenarios_classify() {
        let recorded = scenarios(ADAPTER.name());
        assert_eq!(recorded.len(), 5, "maven fixtures are missing");
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
                "build-error" => RunOutcome::BuildFailed,
                _ => panic!("unknown maven fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
