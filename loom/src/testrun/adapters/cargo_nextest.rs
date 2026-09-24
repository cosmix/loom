//! `cargo nextest run`: recognise invocations, select tests with filtersets,
//! and read nextest's final summary from its progress output.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::shell_quote;
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct CargoNextest;

pub static ADAPTER: CargoNextest = CargoNextest;

const RUN_COMMAND: &[&str] = &["cargo", "nextest", "run"];
const SHORT_COMMAND: &[&str] = &["cargo", "nextest", "r"];

/// Cargo and nextest options that take a separate value before `--`.
const VALUE_FLAGS: &[&str] = &[
    "--manifest-path",
    "-p",
    "--package",
    "--exclude",
    "--bin",
    "--test",
    "--example",
    "--bench",
    "-F",
    "--features",
    "--target",
    "--target-dir",
    "-j",
    "--jobs",
    "--build-jobs",
    "--test-threads",
    "--profile",
    "--cargo-profile",
    "--color",
    "--message-format",
    "--cargo-message-format",
    "--config",
    "--config-file",
    "--user-config-file",
    "--lockfile-path",
    "--status-level",
    "--final-status-level",
    "--failure-output",
    "--success-output",
    "--no-tests",
    "--run-ignored",
    "--retries",
    "--partition",
    "--max-fail",
    "--slow-timeout",
    "--archive-file",
    "--workspace-remap",
    "--filterset",
    "--filter-expr",
    "-E",
    "-C",
    "-Z",
];

/// Filtersets select tests by expression; `--partition` runs one shard.
const FILTER_FLAGS: &[&str] = &["-E", "--filterset", "--filter-expr", "--partition"];

/// Cargo target selectors restrict the test binaries that nextest runs.
const TARGET_FLAGS: &[&str] = &[
    "--lib",
    "--bin",
    "--bins",
    "--test",
    "--example",
    "--examples",
    "--bench",
    "--benches",
    "--doc",
];

static TOTAL: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^\s*Summary \[[^\]\r\n]+\]\s+(\d+) tests? run:"));
static PASSED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^\s*Summary \[[^\]\r\n]+\]\s+\d+ tests? run:[^\r\n]*?\b(\d+) passed\b")
});
static FAILED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^\s*Summary \[[^\]\r\n]+\]\s+\d+ tests? run:[^\r\n]*?\b(\d+) failed\b")
});
static SKIPPED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^\s*Summary \[[^\]\r\n]+\]\s+\d+ tests? run:[^\r\n]*?\b(\d+) skipped\b")
});
static NO_TESTS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\bno tests to run\b"));
static BUILD_ERROR: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^error(\[E\d+\])?: "));

fn nextest_args(argv: &[String]) -> Option<Vec<String>> {
    command_args(argv, RUN_COMMAND).or_else(|| command_args(argv, SHORT_COMMAND))
}

impl TestRunnerAdapter for CargoNextest {
    fn name(&self) -> &'static str {
        "cargo-nextest"
    }

    fn language(&self) -> &'static str {
        "rust"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        nextest_args(argv).is_some()
    }

    /// Positional names, filtersets, partitions and Cargo target selectors
    /// narrow a run.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = nextest_args(argv) else {
            return false;
        };
        let filtered = FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag));
        let targeted = TARGET_FLAGS.iter().any(|flag| has_flag(&args, flag));
        !filtered && !targeted && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, _file: &str, test: &str, _package_dir: &Path) -> String {
        let filter = shell_quote(&format!("test(={test})"));
        format!("cargo nextest run -E {filter}")
    }

    fn select_command(&self, targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        let filters: Vec<String> = targets
            .iter()
            .filter_map(|target| target.name.as_deref())
            .map(|name| format!("test(={name})"))
            .collect();
        if filters.is_empty() {
            return None;
        }
        let filter = shell_quote(&filters.join(" or "));
        Some(format!("cargo nextest run -E {filter}"))
    }

    /// The final `Summary` gives executed tests; nextest may omit zero-valued
    /// passed, failed or skipped counts. A compiler error is a build failure
    /// only when no summary says tests ran.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let total = last_match(&TOTAL, &text);
        if let Some(executed) = total {
            return RunSummary {
                executed: Some(executed),
                passed: Some(last_match(&PASSED, &text).unwrap_or(0)),
                failed: Some(last_match(&FAILED, &text).unwrap_or(0)),
                skipped: Some(last_match(&SKIPPED, &text).unwrap_or(0)),
                build_failed: false,
            };
        }
        if NO_TESTS.is_match(&text) {
            return RunSummary {
                executed: Some(0),
                ..RunSummary::default()
            };
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
    use crate::testrun::{classify, fixture_support, RunOutcome};

    fn words(command: &str) -> Vec<String> {
        command.split_whitespace().map(str::to_string).collect()
    }

    fn target(name: Option<&str>) -> TestTarget {
        TestTarget {
            file: "src/lib.rs".to_string(),
            name: name.map(str::to_string),
        }
    }

    #[test]
    fn single_test_command_uses_exact_filterset() {
        let command =
            ADAPTER.single_test_command("src/lib.rs", "plan::tests::works", Path::new("."));
        assert_eq!(command, "cargo nextest run -E 'test(=plan::tests::works)'");
    }

    #[test]
    fn recognizes_nextest_run_and_short_form() {
        for command in [
            "cargo nextest run",
            "cargo +nightly nextest run --manifest-path loom/Cargo.toml",
            "env CI=1 cargo nextest r",
        ] {
            assert!(ADAPTER.recognizes(&words(command)), "{command}");
        }
        assert!(!ADAPTER.recognizes(&words("cargo test")));
    }

    #[test]
    fn full_run_requires_no_name_filter_or_target() {
        for command in [
            "cargo nextest run",
            "cargo +nightly nextest r --manifest-path loom/Cargo.toml --workspace",
        ] {
            assert!(ADAPTER.is_full_run(&words(command)), "{command}");
        }
        for command in [
            "cargo nextest run plan::tests::works",
            "cargo nextest run -E 'test(=works)'",
            "cargo nextest run --filterset test(=works)",
            "cargo nextest run --filter-expr test(=works)",
            "cargo nextest run --test integration",
            "cargo nextest run --partition count:1/2",
            "cargo nextest run --partition=hash:2/3",
        ] {
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn hostile_test_name_stays_one_filterset_word() {
        let command = ADAPTER.single_test_command("src/lib.rs", HOSTILE_NAME, Path::new("."));
        assert_one_word(&command, &format!("test(={HOSTILE_NAME})"));
        let selected = ADAPTER.select_command(&[target(Some(HOSTILE_NAME))], Path::new("."));
        assert_one_word(
            &selected.expect("a named target"),
            &format!("test(={HOSTILE_NAME})"),
        );
    }

    #[test]
    fn select_command_unites_named_targets() {
        let targets = [target(Some("a::x")), target(None), target(Some("b::y"))];
        assert_eq!(
            ADAPTER.select_command(&targets, Path::new(".")).as_deref(),
            Some("cargo nextest run -E 'test(=a::x) or test(=b::y)'")
        );
        assert_eq!(
            ADAPTER.select_command(&[target(None)], Path::new(".")),
            None
        );
    }

    #[test]
    fn fixture_scenarios_have_expected_outcomes() {
        let scenarios = fixture_support::scenarios(ADAPTER.name());
        assert_eq!(scenarios.len(), 5);
        for scenario in scenarios {
            let (stdout, stderr, exit_code) = fixture_support::load(ADAPTER.name(), &scenario);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            });
            let expected = match scenario.as_str() {
                "build-error" => RunOutcome::BuildFailed,
                "no-match" => RunOutcome::NotSelected,
                "one-pass" => RunOutcome::Passed,
                "one-fail" | "suite" => RunOutcome::Failed,
                other => panic!("unknown fixture: {other}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
