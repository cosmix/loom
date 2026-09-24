//! `cargo test`: the reference adapter the others follow. Layout: a unit struct
//! exposed as `pub static ADAPTER`, output patterns as `LazyLock<Regex>`
//! statics, argv and output work through `crate::testrun::recognize`.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{shell_word, shell_words};
use crate::testrun::recognize::{
    command_args, combined_output, has_flag, pattern, positionals, split_double_dash, sum_matches,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct CargoTest;

pub static ADAPTER: CargoTest = CargoTest;

const COMMAND: &[&str] = &["cargo", "test"];

/// `cargo test` options (before `--`) that take a separate value word.
const CARGO_VALUE_FLAGS: &[&str] = &[
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
    "--profile",
    "--color",
    "--message-format",
    "--config",
    "--lockfile-path",
    "-C",
    "-Z",
];

/// libtest options (after `--`) that take a separate value word.
const LIBTEST_VALUE_FLAGS: &[&str] = &[
    "--skip",
    "--test-threads",
    "--format",
    "--color",
    "--logfile",
    "--shuffle-seed",
    "-Z",
];

/// `cargo test` options that run only the named or listed targets.
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

/// libtest options that run a subset of the tests a filter would select.
const SUBSET_FLAGS: &[&str] = &["--skip", "--ignored"];

/// One per test binary; the count includes ignored tests.
static RUNNING: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^running (\d+) tests?\b"));
static PASSED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^test result: \w+\. (\d+) passed;"));
static FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^test result: .* (\d+) failed;"));
static IGNORED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^test result: .* (\d+) ignored;"));
/// A compiler diagnostic line: `error: ...` or `error[E0425]: ...`.
static BUILD_ERROR: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^error(\[E\d+\])?: "));

impl TestRunnerAdapter for CargoTest {
    fn name(&self) -> &'static str {
        "cargo-test"
    }

    fn language(&self) -> &'static str {
        "rust"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// Full when no test-name filter (a positional before or after `--`), no
    /// target flag and no libtest subset flag is present. `--all-targets` and
    /// `--workspace` stay full runs.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        let (cargo_args, libtest_args) = split_double_dash(&args);
        let targeted = TARGET_FLAGS.iter().any(|f| has_flag(cargo_args, f))
            || SUBSET_FLAGS.iter().any(|f| has_flag(libtest_args, f));
        !targeted
            && positionals(cargo_args, CARGO_VALUE_FLAGS).is_empty()
            && positionals(libtest_args, LIBTEST_VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, _file: &str, test: &str, _package_dir: &Path) -> String {
        format!("cargo test {} -- --exact", shell_word(test))
    }

    fn select_command(&self, targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        let names: Vec<&str> = targets.iter().filter_map(|t| t.name.as_deref()).collect();
        if names.is_empty() {
            return None;
        }
        Some(format!("cargo test -- {}", shell_words(names)))
    }

    /// Sums every test binary's `running N tests` and `test result:` line.
    /// `executed` is the running total less ignored tests, so a binary that
    /// crashed before its `test result:` line still counts. A compiler error
    /// before any binary started is a build failure; one printed by a running
    /// test (a failing doctest) is a test failure.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let running = sum_matches(&RUNNING, &text);
        let skipped = sum_matches(&IGNORED, &text);
        let executed = running.map(|n| n.saturating_sub(skipped.unwrap_or(0)));
        RunSummary {
            executed,
            passed: sum_matches(&PASSED, &text),
            failed: sum_matches(&FAILED, &text),
            skipped,
            build_failed: running.is_none() && BUILD_ERROR.is_match(&text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testrun::command::{assert_one_word, HOSTILE_NAME};

    fn target(name: Option<&str>) -> TestTarget {
        TestTarget {
            file: "src/lib.rs".to_string(),
            name: name.map(str::to_string),
        }
    }

    #[test]
    fn single_test_command_runs_the_exact_libtest_path() {
        let dir = Path::new(".");
        let command = ADAPTER.single_test_command("src/lib.rs", "a::b", dir);
        assert_eq!(command, "cargo test a::b -- --exact");
    }

    #[test]
    fn select_command_filters_by_named_targets_only() {
        let dir = Path::new(".");
        let targets = [target(Some("a::x")), target(None), target(Some("b::y"))];
        let command = ADAPTER.select_command(&targets, dir);
        assert_eq!(command.as_deref(), Some("cargo test -- a::x b::y"));
        assert_eq!(ADAPTER.select_command(&[target(None)], dir), None);
    }

    #[test]
    fn hostile_test_name_stays_one_word() {
        let dir = Path::new(".");
        let command = ADAPTER.single_test_command("src/lib.rs", HOSTILE_NAME, dir);
        assert_one_word(&command, HOSTILE_NAME);
        let selected = ADAPTER.select_command(&[target(Some(HOSTILE_NAME))], dir);
        assert_one_word(&selected.expect("a named target"), HOSTILE_NAME);
    }

    #[test]
    fn compiler_diagnostic_lines_mark_a_build_failure() {
        let diagnostic = stderr_only("error[E0425]: cannot find value `x` in this scope\n");
        assert!(ADAPTER.parse(&diagnostic).build_failed);
        let log = stderr_only("warning: could not compile template, see error[E0308] above\n");
        assert!(!ADAPTER.parse(&log).build_failed);
    }

    fn stderr_only(stderr: &str) -> RunOutput<'_> {
        RunOutput {
            stdout: "",
            stderr,
            exit_code: Some(101),
        }
    }
}
