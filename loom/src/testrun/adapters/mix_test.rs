//! `mix test`: reads ExUnit's final counts and detects compile failures.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{shell_quote, shell_word};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals, summary_from_counts,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct MixTest;

pub static ADAPTER: MixTest = MixTest;

const COMMAND: &[&str] = &["mix", "test"];

/// Mix options whose following word is a value rather than a test file.
const VALUE_FLAGS: &[&str] = &[
    "--only",
    "--exclude",
    "--include",
    "--seed",
    "--max-cases",
    "--max-failures",
    "--timeout",
    "--slowest",
    "--formatter",
    "--partition",
    "--partitions",
];

/// Filters that prevent the invocation from representing the whole suite.
const FILTER_FLAGS: &[&str] = &["--only", "--exclude", "--include", "--failed", "--stale"];

static TOTAL: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^[ \t]*(\d+) tests?, \d+ failures?(?:, \d+ excluded)?[ \t]*\r?$")
});
static FAILED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^[ \t]*\d+ tests?, (\d+) failures?(?:, \d+ excluded)?[ \t]*\r?$")
});
static EXCLUDED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^[ \t]*\d+ tests?, \d+ failures?, (\d+) excluded[ \t]*\r?$"));
static FINISHED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^[ \t]*Finished in \d"));
static BUILD_ERROR: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^== Compilation error|\*\* \(CompileError\)"));

impl TestRunnerAdapter for MixTest {
    fn name(&self) -> &'static str {
        "mix-test"
    }

    fn language(&self) -> &'static str {
        "elixir"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// File and line targets, tag filters, and rerun flags select a subset.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        !FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag))
            && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let tag = shell_quote(&format!("test:{test}"));
        format!("mix test {} --only {tag}", shell_word(file))
    }

    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }

    /// ExUnit's test count includes excluded tests, which did not execute.
    /// A compiler diagnostic before the finished marker means no test ran.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        if !FINISHED.is_match(&text) && BUILD_ERROR.is_match(&text) {
            return RunSummary {
                build_failed: true,
                ..RunSummary::default()
            };
        }
        let Some(total) = last_match(&TOTAL, &text) else {
            return RunSummary::default();
        };
        let failed = last_match(&FAILED, &text).unwrap_or_default();
        let excluded = last_match(&EXCLUDED, &text).unwrap_or_default();
        let executed = total.saturating_sub(excluded);
        summary_from_counts(
            Some(executed.saturating_sub(failed)),
            Some(failed),
            Some(excluded),
        )
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
    fn named_exunit_test_uses_its_full_test_tag() {
        let command =
            ADAPTER.single_test_command("test/alpha_test.exs", "test alpha passes", Path::new("."));
        assert_eq!(
            command,
            "mix test test/alpha_test.exs --only 'test:test alpha passes'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_tag_word() {
        let command = ADAPTER.single_test_command("a_test.exs", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &format!("test:{HOSTILE_NAME}"));
    }

    #[test]
    fn mix_test_invocations_are_recognized() {
        assert!(ADAPTER.recognizes(&words("mix test")));
        assert!(ADAPTER.recognizes(&words("env MIX_ENV=test mix test test/alpha_test.exs")));
        assert!(!ADAPTER.recognizes(&words("mix compile")));
    }

    #[test]
    fn mix_filters_and_files_narrow_the_run() {
        assert!(ADAPTER.is_full_run(&words("mix test")));
        assert!(ADAPTER.is_full_run(&words("mix test --seed 12 --max-cases=4")));
        assert!(!ADAPTER.is_full_run(&words("mix compile")));
        for command in [
            "mix test --only test:alpha",
            "mix test --exclude slow",
            "mix test --include slow",
            "mix test --failed",
            "mix test --stale",
            "mix test test/alpha_test.exs",
            "mix test test/alpha_test.exs:12",
        ] {
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn mix_test_has_no_multi_target_command() {
        let targets = [TestTarget {
            file: "test/alpha_test.exs".to_string(),
            name: Some("test alpha passes".to_string()),
        }];
        assert_eq!(ADAPTER.select_command(&targets, Path::new(".")), None);
    }

    #[test]
    fn exunit_fixtures_classify_and_count_runs() {
        let recorded = scenarios(ADAPTER.name());
        assert_eq!(
            recorded,
            vec![
                "build-error".to_string(),
                "no-match".to_string(),
                "no-match.excluded".to_string(),
                "one-fail".to_string(),
                "one-pass".to_string(),
                "suite".to_string(),
            ]
        );
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
                _ => panic!("unknown mix-test fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
            if scenario.starts_with("no-match") {
                assert_eq!(summary.executed, Some(0), "{scenario}");
            }
        }
    }
}
