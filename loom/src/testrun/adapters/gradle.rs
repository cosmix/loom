//! `gradle test`: recognises Gradle test tasks and reads plain console output.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{marker_choice, shell_quote};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Gradle;

pub static ADAPTER: Gradle = Gradle;

/// Gradle options whose following word is a value, not a task name.
const VALUE_FLAGS: &[&str] = &[
    "--tests",
    "-p",
    "--project-dir",
    "-b",
    "--build-file",
    "-c",
    "--settings-file",
    "-I",
    "--init-script",
    "-g",
    "--gradle-user-home",
    "--include-build",
    "--max-workers",
    "--console",
    "--warning-mode",
    "--configuration-cache-problems",
];

static COMPLETED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)\b(\d+) tests? completed, \d+ failed\b"));
static FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)\b\d+ tests? completed, (\d+) failed\b"));
static NO_MATCH: LazyLock<Regex> = LazyLock::new(|| pattern(r"No tests found for given includes"));
static NO_SOURCE: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^> Task :(?:[^\s:]+:)*test NO-SOURCE\r?$"));
static TEST_TASK: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^> Task :(?:[^\s:]+:)*test[ \t]*\r?$"));
static BUILD_SUCCESSFUL: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^BUILD SUCCESSFUL\b"));
/// The cause Gradle's `FAILURE: Build failed` banner gives under
/// `* What went wrong:` when a compile task failed.
static COMPILATION_FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^> Compilation failed\b"));

/// The arguments after any supported Gradle executable, with shared prefix handling.
fn gradle_args(argv: &[String]) -> Option<Vec<String>> {
    ["gradle", "gradlew", "gradlew.bat"]
        .iter()
        .find_map(|name| command_args(argv, &[*name]))
}

fn is_test_task(word: &str) -> bool {
    if word == "test" {
        return true;
    }
    word.strip_prefix(':').is_some_and(|path| {
        path.rsplit(':').next() == Some("test") && path.split(':').all(|part| !part.is_empty())
    })
}

impl TestRunnerAdapter for Gradle {
    fn name(&self) -> &'static str {
        "gradle"
    }

    fn language(&self) -> &'static str {
        "java"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        gradle_args(argv).is_some_and(|args| {
            positionals(&args, VALUE_FLAGS)
                .into_iter()
                .any(is_test_task)
        })
    }

    /// A `--tests` name filter excludes the rest of the suite.
    fn is_full_run(&self, argv: &[String]) -> bool {
        self.recognizes(argv) && gradle_args(argv).is_some_and(|args| !has_flag(&args, "--tests"))
    }

    fn single_test_command(&self, _file: &str, test: &str, package_dir: &Path) -> String {
        let runner = marker_choice(package_dir, &["gradlew"], "./gradlew", "gradle");
        format!("{runner} test --tests {}", shell_quote(test))
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// Gradle prints an exact count on test failure, but no count on success.
    /// A successful plain `> Task :test` establishes `executed: Some(1)` only
    /// as a lower bound; `NO-SOURCE` and unmatched `--tests` explicitly mean 0.
    /// A result wins over build-failure evidence, which a test's own log
    /// output can resemble.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if let (Some(total), Some(failed)) =
            (last_match(&COMPLETED, &text), last_match(&FAILED, &text))
        {
            return summary_from_counts(Some(total.saturating_sub(failed)), Some(failed), None);
        }
        if BUILD_SUCCESSFUL.is_match(&text) && TEST_TASK.is_match(&text) {
            return RunSummary {
                executed: Some(1),
                failed: Some(0),
                ..RunSummary::default()
            };
        }
        if NO_MATCH.is_match(&text) {
            return summary_from_counts(Some(0), Some(0), None);
        }
        if COMPILATION_FAILED.is_match(&text) {
            return RunSummary {
                build_failed: true,
                ..RunSummary::default()
            };
        }
        if NO_SOURCE.is_match(&text) {
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
    fn named_java_method_uses_wrapper_when_present() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        std::fs::write(dir.path().join("gradlew"), "").expect("wrapper file");
        let command = ADAPTER.single_test_command(
            "src/test/AlphaTest.java",
            "sample.AlphaTest.ok",
            dir.path(),
        );
        assert_eq!(command, "./gradlew test --tests 'sample.AlphaTest.ok'");
    }

    #[test]
    fn named_java_method_uses_system_gradle_without_wrapper() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let command = ADAPTER.single_test_command(
            "src/test/AlphaTest.java",
            "sample.AlphaTest.ok",
            dir.path(),
        );
        assert_eq!(command, "gradle test --tests 'sample.AlphaTest.ok'");
    }

    #[test]
    fn hostile_test_name_stays_one_word() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let command = ADAPTER.single_test_command("A.java", HOSTILE_NAME, dir.path());
        assert_one_word(&command, HOSTILE_NAME);
    }

    #[test]
    fn test_log_mentioning_compilation_failure_keeps_the_result() {
        let passing = RunOutput {
            stdout: "> Task :test\n\
                     AlphaTest > ok STANDARD_OUT\n    \
                     compilation failed for the template under test\n\
                     > Compilation failed on purpose\n\n\
                     BUILD SUCCESSFUL in 1s\n",
            stderr: "",
            exit_code: Some(0),
        };
        let summary = ADAPTER.parse(&passing);
        assert!(!summary.build_failed);
        assert_eq!(classify(&summary, passing.exit_code), RunOutcome::Passed);
        let failing = RunOutput {
            stdout: "> Compilation failed in a test log\n3 tests completed, 1 failed\n",
            stderr: "",
            exit_code: Some(1),
        };
        let summary = ADAPTER.parse(&failing);
        assert_eq!(classify(&summary, failing.exit_code), RunOutcome::Failed);
    }

    #[test]
    fn test_tasks_are_recognized_for_gradle_executables() {
        assert!(ADAPTER.recognizes(&words("./gradlew test")));
        assert!(ADAPTER.recognizes(&words("gradle :app:test")));
        assert!(ADAPTER.recognizes(&words("gradlew.bat :app:core:test")));
        assert!(!ADAPTER.recognizes(&words("gradle build")));
        assert!(!ADAPTER.recognizes(&words("gradle build --tests test")));
    }

    #[test]
    fn tests_filter_marks_run_as_partial() {
        assert!(ADAPTER.is_full_run(&words("./gradlew test")));
        assert!(ADAPTER.is_full_run(&words("gradle :app:test")));
        assert!(!ADAPTER.is_full_run(&words("gradle test --tests sample.AlphaTest.ok")));
        assert!(!ADAPTER.is_full_run(&words("gradle test --tests=sample.AlphaTest.ok")));
        assert!(!ADAPTER.is_full_run(&words("gradle build")));
    }

    #[test]
    fn selection_by_file_is_unavailable() {
        let targets = [TestTarget {
            file: "src/test/AlphaTest.java".to_string(),
            name: Some("sample.AlphaTest.ok".to_string()),
        }];
        assert_eq!(ADAPTER.select_command(&targets, Path::new(".")), None);
    }

    #[test]
    fn successful_task_without_count_has_one_test_lower_bound() {
        let output = RunOutput {
            stdout: "> Task :app:test\n\nBUILD SUCCESSFUL in 1s\n",
            stderr: "",
            exit_code: Some(0),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!(summary.executed, Some(1));
        assert_eq!(summary.passed, None);
        assert_eq!(classify(&summary, output.exit_code), RunOutcome::Passed);
    }

    #[test]
    fn no_source_and_exit_only_do_not_claim_a_test_ran() {
        let no_source = RunOutput {
            stdout: "> Task :test NO-SOURCE\n\nBUILD SUCCESSFUL in 1s\n",
            stderr: "",
            exit_code: Some(0),
        };
        assert_eq!(ADAPTER.parse(&no_source).executed, Some(0));
        let exit_only = RunOutput {
            stdout: "BUILD SUCCESSFUL in 1s\n",
            stderr: "",
            exit_code: Some(0),
        };
        assert_eq!(ADAPTER.parse(&exit_only).executed, None);
    }

    #[test]
    fn documented_gradle_scenarios_classify() {
        let recorded = scenarios(ADAPTER.name());
        assert_eq!(recorded.len(), 5, "gradle fixtures are incomplete");
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
                _ => panic!("unknown gradle fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
