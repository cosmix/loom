//! `go test`: selects packages by file directory and reads verbose test results.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{distinct, regex_literal, shell_quote, shell_word, shell_words};
use crate::testrun::recognize::{
    combined_output, command_args, count_matches, has_flag, pattern, positionals,
    summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct GoTest;

pub static ADAPTER: GoTest = GoTest;

const COMMAND: &[&str] = &["go", "test"];

/// `go test` options whose values must not be mistaken for package operands.
const VALUE_FLAGS: &[&str] = &[
    "-run",
    "-skip",
    "-bench",
    "-fuzz",
    "-list",
    "-count",
    "-timeout",
    "-parallel",
    "-cpu",
    "-tags",
    "-mod",
    "-modfile",
    "-vet",
    "-exec",
    "-coverprofile",
    "-coverpkg",
    "-covermode",
    "-coverdir",
    "-gcflags",
    "-ldflags",
    "-test.run",
    "-test.skip",
];

/// Options that select tests or change a run into a listing or fuzz run.
const FILTER_FLAGS: &[&str] = &[
    "-run",
    "-skip",
    "-bench",
    "-fuzz",
    "-list",
    "-test.run",
    "-test.skip",
];

/// Top-level results start at column 0; `t.Run` subtest results are indented.
static PASSED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^--- PASS: "));
static FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^--- FAIL: "));
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^--- SKIP: "));
static NO_MATCH: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"\[no tests to run\]|testing: warning: no tests to run"));
static BUILD_ERROR: LazyLock<Regex> = LazyLock::new(|| pattern(r"build failed|\[setup failed\]"));

/// A file's Go package path, relative to the command's working directory.
fn package_path(file: &str, package_dir: &Path) -> String {
    let path = Path::new(file);
    let relative = path.strip_prefix(package_dir).unwrap_or(path);
    let directory = relative.parent().unwrap_or(Path::new(""));
    let directory = directory.strip_prefix(".").unwrap_or(directory);
    if directory.as_os_str().is_empty() {
        "./".to_string()
    } else {
        format!("./{}/", directory.display())
    }
}

impl TestRunnerAdapter for GoTest {
    fn name(&self) -> &'static str {
        "go-test"
    }

    fn language(&self) -> &'static str {
        "go"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// The entire package tree (or the current package) with no test filter.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        if FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag)) {
            return false;
        }
        positionals(&args, VALUE_FLAGS)
            .iter()
            .all(|package| matches!(*package, "." | "./" | "./..."))
    }

    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String {
        let package = package_path(file, package_dir);
        let package = shell_word(&package);
        let filter = shell_quote(&format!("^{}$", regex_literal(test)));
        format!("go test {package} -run {filter} -v")
    }

    /// Select every target's package once; `go test` cannot combine names
    /// from different packages in one `-run` expression without losing tests.
    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String> {
        let packages = distinct(
            targets
                .iter()
                .map(|target| package_path(&target.file, package_dir)),
        );
        let packages = shell_words(packages.iter().map(String::as_str));
        (!packages.is_empty()).then(|| format!("go test {packages}"))
    }

    /// Verbose output prints one `--- PASS:` or `--- FAIL:` per completed
    /// top-level test; subtests do not add to the count. A no-match warning
    /// matters even when `go test` exits successfully.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let passed = count_matches(&PASSED, &text);
        let failed = count_matches(&FAILED, &text);
        let skipped = count_matches(&SKIPPED, &text);
        let saw_result = passed > 0 || failed > 0 || skipped > 0 || NO_MATCH.is_match(&text);
        let mut summary = if saw_result {
            summary_from_counts(Some(passed), Some(failed), (skipped > 0).then_some(skipped))
        } else {
            RunSummary::default()
        };
        summary.build_failed = BUILD_ERROR.is_match(&text);
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
    fn exact_go_function_command_uses_its_package() {
        let command =
            ADAPTER.single_test_command("pkg/math/add_test.go", "TestAlphaPasses", Path::new("."));
        assert_eq!(command, "go test ./pkg/math/ -run '^TestAlphaPasses$' -v");
        assert_eq!(
            ADAPTER.single_test_command("add_test.go", "TestRoot", Path::new(".")),
            "go test ./ -run '^TestRoot$' -v"
        );
    }

    #[test]
    fn go_invocations_are_identified_through_environment_prefix() {
        assert!(ADAPTER.recognizes(&words("go test ./... -v")));
        assert!(ADAPTER.recognizes(&words("env CI=1 go test ./...")));
        assert!(!ADAPTER.recognizes(&words("go version")));
        assert!(!ADAPTER.recognizes(&words("echo go test")));
    }

    #[test]
    fn go_package_tree_without_filter_is_full() {
        assert!(ADAPTER.is_full_run(&words("go test ./... -count=1 -v")));
        assert!(ADAPTER.is_full_run(&words("go test")));
        assert!(!ADAPTER.is_full_run(&words("go test ./... -run TestAlpha")));
        assert!(!ADAPTER.is_full_run(&words("go test ./pkg/")));
    }

    #[test]
    fn go_selection_deduplicates_package_directories() {
        let targets = [
            TestTarget {
                file: "d1/a_test.go".to_string(),
                name: Some("TestA".to_string()),
            },
            TestTarget {
                file: "d1/b_test.go".to_string(),
                name: None,
            },
            TestTarget {
                file: "d2/c_test.go".to_string(),
                name: Some("TestC".to_string()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, Path::new(".")),
            Some("go test ./d1/ ./d2/".to_string())
        );
        assert_eq!(ADAPTER.select_command(&[], Path::new(".")), None);
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("a_test.go", HOSTILE_NAME, Path::new("."));
        let expected = format!("^{}$", regex_literal(HOSTILE_NAME));
        assert_one_word(&command, &expected);
    }

    #[test]
    fn subtests_do_not_count_as_tests() {
        let output = RunOutput {
            stdout: "=== RUN   TestParent\n\
                     === RUN   TestParent/one\n\
                     === RUN   TestParent/two\n\
                     --- PASS: TestParent (0.00s)\n    \
                     --- PASS: TestParent/one (0.00s)\n    \
                     --- PASS: TestParent/two (0.00s)\n\
                     PASS\n",
            stderr: "",
            exit_code: Some(0),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!((summary.executed, summary.passed), (Some(1), Some(1)));
    }

    #[test]
    fn recorded_go_output_produces_each_verdict() {
        let recorded = scenarios(ADAPTER.name());
        assert!(!recorded.is_empty(), "go-test fixtures are missing");
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
                _ => panic!("unknown go-test fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
