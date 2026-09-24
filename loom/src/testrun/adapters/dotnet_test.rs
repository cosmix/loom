//! `dotnet test`: reads VSTest summaries from each test project in a run.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::shell_quote;
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, pattern, positionals, sum_matches, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct DotnetTest;

pub static ADAPTER: DotnetTest = DotnetTest;

const COMMAND: &[&str] = &["dotnet", "test"];

/// Options whose separate values are not project or assembly operands.
const VALUE_FLAGS: &[&str] = &[
    "--filter",
    "--framework",
    "-f",
    "--configuration",
    "-c",
    "--runtime",
    "-r",
    "--output",
    "-o",
    "--logger",
    "-l",
    "--settings",
    "-s",
    "--results-directory",
    "--test-adapter-path",
    "--collect",
    "--arch",
    "--os",
    "--property",
    "-p",
    "--verbosity",
    "-v",
];

/// One `Passed!` or `Failed!` line is emitted per test assembly.
static FAILED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^[ \t]*(?:Passed|Failed)![ \t]*-[ \t]*Failed:[ \t]*(\d+),[ \t]*Passed:[ \t]*\d+,[ \t]*Skipped:[ \t]*\d+,[ \t]*Total:",
    )
});
static PASSED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^[ \t]*(?:Passed|Failed)![ \t]*-[ \t]*Failed:[ \t]*\d+,[ \t]*Passed:[ \t]*(\d+),[ \t]*Skipped:[ \t]*\d+,[ \t]*Total:",
    )
});
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?m)^[ \t]*(?:Passed|Failed)![ \t]*-[ \t]*Failed:[ \t]*\d+,[ \t]*Passed:[ \t]*\d+,[ \t]*Skipped:[ \t]*(\d+),[ \t]*Total:",
    )
});
static NO_MATCH: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"No test matches the given testcase filter"));
static BUILD_ERROR: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\berror CS\d+\b|\bBuild FAILED\b"));

impl TestRunnerAdapter for DotnetTest {
    fn name(&self) -> &'static str {
        "dotnet-test"
    }

    fn language(&self) -> &'static str {
        "csharp"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A project or solution operand still runs its whole suite. An assembly
    /// operand, a test filter, and a listing command do not.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        !has_flag(&args, "--filter")
            && !has_flag(&args, "--list-tests")
            && positionals(&args, VALUE_FLAGS)
                .iter()
                .all(|operand| !operand.to_ascii_lowercase().ends_with(".dll"))
    }

    /// The filter is single-quoted: inside double quotes `sh` would still
    /// expand a `$` or backtick in the name.
    fn single_test_command(&self, _file: &str, test: &str, _package_dir: &Path) -> String {
        let filter = shell_quote(&format!("FullyQualifiedName={test}"));
        format!("dotnet test --filter {filter}")
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// Sum each project's summary; a no-match or compilation error only sets
    /// the result when no project reported a test summary.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let mut summary = summary_from_counts(
            sum_matches(&PASSED, &text),
            sum_matches(&FAILED, &text),
            sum_matches(&SKIPPED, &text),
        );
        if summary.executed.is_some() {
            return summary;
        }
        if BUILD_ERROR.is_match(&text) {
            summary.build_failed = true;
        } else if NO_MATCH.is_match(&text) {
            summary.executed = Some(0);
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};
    use crate::testrun::fixture_support;
    use crate::testrun::{classify, RunOutcome};

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn command_selects_fully_qualified_name() {
        assert_eq!(
            ADAPTER.single_test_command(
                "UnitTest1.cs",
                "FixtureDotnet.FixtureTests.AlphaPasses",
                Path::new("."),
            ),
            "dotnet test --filter 'FullyQualifiedName=FixtureDotnet.FixtureTests.AlphaPasses'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_filter_word() {
        let command = ADAPTER.single_test_command("UnitTest1.cs", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &format!("FullyQualifiedName={HOSTILE_NAME}"));
    }

    #[test]
    fn recognizes_dotnet_test_invocations() {
        assert!(ADAPTER.recognizes(&argv(&["dotnet", "test"])));
        assert!(ADAPTER.recognizes(&argv(&["DOTNET_NOLOGO=1", "dotnet", "test"])));
        assert!(!ADAPTER.recognizes(&argv(&["dotnet", "build"])));
    }

    #[test]
    fn full_run_requires_no_test_filter_or_assembly() {
        assert!(ADAPTER.is_full_run(&argv(&["dotnet", "test"])));
        assert!(ADAPTER.is_full_run(&argv(&["dotnet", "test", "FixtureDotnet.csproj"])));
        assert!(ADAPTER.is_full_run(&argv(&["dotnet", "test", "Fixture.sln"])));
        assert!(!ADAPTER.is_full_run(&argv(&["dotnet", "test", "--filter", "Name=Alpha"])));
        assert!(!ADAPTER.is_full_run(&argv(&["dotnet", "test", "--filter=Name=Alpha"])));
        assert!(!ADAPTER.is_full_run(&argv(&["dotnet", "test", "FixtureDotnet.dll"])));
        assert!(!ADAPTER.is_full_run(&argv(&["dotnet", "test", "--list-tests"])));
        assert!(!ADAPTER.is_full_run(&argv(&["dotnet", "build"])));
    }

    #[test]
    fn selection_command_is_unavailable() {
        let targets = [TestTarget {
            file: "UnitTest1.cs".to_string(),
            name: Some("FixtureDotnet.FixtureTests.AlphaPasses".to_string()),
        }];
        assert_eq!(ADAPTER.select_command(&targets, Path::new(".")), None);
    }

    #[test]
    fn recorded_scenarios_parse_and_classify() {
        let scenarios = fixture_support::scenarios(ADAPTER.name());
        assert!(!scenarios.is_empty());
        for scenario in scenarios {
            let (stdout, stderr, exit_code) = fixture_support::load(ADAPTER.name(), &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let expected = match scenario.split('.').next() {
                Some("one-pass") => RunOutcome::Passed,
                Some("one-fail" | "suite") => RunOutcome::Failed,
                Some("no-match") => RunOutcome::NotSelected,
                Some("build-error") => RunOutcome::BuildFailed,
                other => panic!("unexpected dotnet fixture scenario: {other:?}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }

    #[test]
    fn solution_sums_all_project_summaries() {
        let output = RunOutput {
            stdout: "Passed! - Failed: 0, Passed: 2, Skipped: 1, Total: 3\n\
                     Failed! - Failed: 1, Passed: 1, Skipped: 0, Total: 2\n",
            stderr: "",
            exit_code: Some(1),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!(summary.executed, Some(4));
        assert_eq!(summary.passed, Some(3));
        assert_eq!(summary.failed, Some(1));
        assert_eq!(summary.skipped, Some(1));
        assert_eq!(classify(&summary, output.exit_code), RunOutcome::Failed);
    }
}
