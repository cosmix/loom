//! `rspec`: selects Ruby examples and reads the progress formatter summary.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{
    distinct_files, exact_ruby_regex, marker_choice, shell_quote, shell_word, shell_words,
};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Rspec;

pub static ADAPTER: Rspec = Rspec;

const COMMAND: &[&str] = &["rspec"];

/// Options whose following word is a value, not a file selection.
const VALUE_FLAGS: &[&str] = &[
    "-e",
    "--example",
    "-E",
    "--example-matches",
    "-t",
    "--tag",
    "-I",
    "--require",
    "-r",
    "--format",
    "-f",
    "--out",
    "-o",
    "--order",
    "--seed",
    "--default-path",
    "--pattern",
    "-P",
    "--exclude-pattern",
    "--options",
    "-O",
];

const FILTER_FLAGS: &[&str] = &[
    "-e",
    "--example",
    "-E",
    "--example-matches",
    "-t",
    "--tag",
    "--only-failures",
    "--next-failure",
    "--pattern",
    "-P",
    "--exclude-pattern",
    "--bisect",
];

static EXAMPLES: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^\s*(\d+) examples?, \d+ failures?\b"));
static FAILURES: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^\s*\d+ examples?, (\d+) failures?\b"));
static PENDING: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^\s*\d+ examples?, \d+ failures?, (\d+) pending\b"));
static OUTSIDE_ERROR: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"errors? occurred outside of examples"));

fn runner(package_dir: &Path) -> &'static str {
    marker_choice(package_dir, &["Gemfile"], "bundle exec rspec", "rspec")
}

impl TestRunnerAdapter for Rspec {
    fn name(&self) -> &'static str {
        "rspec"
    }

    fn language(&self) -> &'static str {
        "ruby"
    }

    /// RSpec reads `.rspec` and the Bundler `Gemfile` from the cwd.
    fn run_root_markers(&self) -> &'static [&'static str] {
        &[".rspec", "Gemfile"]
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        command_args(argv, COMMAND).is_some()
    }

    /// A file operand or an example, tag, or failure filter selects a subset.
    fn is_full_run(&self, argv: &[String]) -> bool {
        let Some(args) = command_args(argv, COMMAND) else {
            return false;
        };
        !FILTER_FLAGS.iter().any(|flag| has_flag(&args, flag))
            && positionals(&args, VALUE_FLAGS).is_empty()
    }

    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String {
        let file = shell_word(file);
        let name = shell_quote(&exact_ruby_regex(test));
        format!("{} {file} -E {name}", runner(package_dir))
    }

    /// RSpec accepts all selected files in one invocation; names are omitted
    /// because an `-e` filter would apply to every file in the selection.
    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String> {
        let files = distinct_files(targets);
        (!files.is_empty()).then(|| format!("{} {}", runner(package_dir), shell_words(files)))
    }

    /// The progress formatter includes pending examples in its example total.
    /// A load error outside examples is a build failure when nothing ran.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let examples = last_match(&EXAMPLES, &text);
        let failed = last_match(&FAILURES, &text);
        let pending = last_match(&PENDING, &text);
        let executed = examples.map(|count| count.saturating_sub(pending.unwrap_or(0)));
        RunSummary {
            executed,
            passed: executed.map(|count| count.saturating_sub(failed.unwrap_or(0))),
            failed,
            skipped: pending,
            build_failed: examples == Some(0) && OUTSIDE_ERROR.is_match(&text),
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
    fn example_command_uses_bundle_when_gemfile_exists() {
        let dir = tempfile::tempdir().unwrap();
        let file = "spec/widget_spec.rb";
        let test = "Widget returns a value";
        assert_eq!(
            ADAPTER.single_test_command(file, test, dir.path()),
            r"rspec spec/widget_spec.rb -E '\AWidget returns a value\z'"
        );
        std::fs::write(
            dir.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .unwrap();
        assert_eq!(
            ADAPTER.single_test_command(file, test, dir.path()),
            r"bundle exec rspec spec/widget_spec.rb -E '\AWidget returns a value\z'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_example_word() {
        let dir = tempfile::tempdir().unwrap();
        let command = ADAPTER.single_test_command("a_spec.rb", HOSTILE_NAME, dir.path());
        assert_one_word(&command, &exact_ruby_regex(HOSTILE_NAME));
    }

    #[test]
    fn installed_rspec_rejects_exact_name_collisions() {
        let Some(runner) = std::env::var_os("LOOM_TEST_RSPEC_BIN") else {
            return;
        };
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/testrun/fixtures/rspec/collision_spec.rb");
        let dir = tempfile::Builder::new()
            .prefix("ver01-")
            .tempdir_in(fixture.parent().expect("RSpec fixture directory"))
            .expect("temporary RSpec project");
        std::fs::copy(&fixture, dir.path().join("collision_spec.rb"))
            .expect("copy collision fixture");

        for (test, skip_exact, expected) in [
            ("suite value (a)+", false, RunOutcome::Passed),
            ("suite value (a)+", true, RunOutcome::NotSelected),
            ("suite absent (a)+", false, RunOutcome::NotSelected),
        ] {
            let mut command = std::process::Command::new(&runner);
            command
                .args(["collision_spec.rb", "-E"])
                .arg(exact_ruby_regex(test))
                .current_dir(dir.path());
            if skip_exact {
                command.env("VER01_SKIP_EXACT", "1");
            }
            let output = command.output().expect("run installed RSpec");
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let summary = ADAPTER.parse(&RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code: output.status.code(),
            });
            assert_eq!(
                classify(&summary, output.status.code()),
                expected,
                "{stdout}\n{stderr}"
            );
            assert_eq!(
                summary.executed,
                Some(if expected == RunOutcome::Passed { 1 } else { 0 })
            );
        }
    }

    #[test]
    fn rspec_invocations_include_bundle_and_bin_path() {
        assert!(ADAPTER.recognizes(&words("rspec spec/widget_spec.rb")));
        assert!(ADAPTER.recognizes(&words("bin/rspec spec/widget_spec.rb")));
        assert!(ADAPTER.recognizes(&words("bundle exec rspec spec/widget_spec.rb")));
        assert!(!ADAPTER.recognizes(&words("bundle exec rake spec")));
    }

    #[test]
    fn suite_invocation_rejects_example_and_file_filters() {
        assert!(ADAPTER.is_full_run(&words("rspec --format progress")));
        assert!(ADAPTER.is_full_run(&words("bundle exec rspec")));
        for command in [
            "rspec spec/widget_spec.rb",
            "rspec spec/widget_spec.rb:12",
            "rspec -e Widget",
            "rspec --example=Widget",
            "rspec -E Widget",
            "rspec --example-matches=Widget",
            "rspec -t fast",
            "rspec --tag=fast",
            "rspec --only-failures",
            "rspec --next-failure",
        ] {
            assert!(!ADAPTER.is_full_run(&words(command)), "{command}");
        }
    }

    #[test]
    fn selection_passes_distinct_files_to_one_command() {
        let dir = tempfile::tempdir().unwrap();
        let targets = [
            TestTarget {
                file: "spec/a_spec.rb".into(),
                name: Some("a".into()),
            },
            TestTarget {
                file: "spec/b_spec.rb".into(),
                name: None,
            },
            TestTarget {
                file: "spec/a_spec.rb".into(),
                name: Some("other".into()),
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("rspec spec/a_spec.rb spec/b_spec.rb")
        );
        assert_eq!(ADAPTER.select_command(&[], dir.path()), None);
        std::fs::write(dir.path().join("Gemfile"), "").unwrap();
        assert_eq!(
            ADAPTER.select_command(&targets, dir.path()).as_deref(),
            Some("bundle exec rspec spec/a_spec.rb spec/b_spec.rb")
        );
    }

    #[test]
    fn documented_progress_scenarios_have_expected_outcomes() {
        let recorded = scenarios(ADAPTER.name());
        assert_eq!(recorded.len(), 4);
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
                _ => panic!("unknown rspec fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario == "suite" {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }

    #[test]
    fn pending_examples_do_not_count_as_executed() {
        let output = RunOutput {
            stdout: "Finished in 0.001 seconds\n3 examples, 1 failure, 1 pending\n",
            stderr: "",
            exit_code: Some(1),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!(
            (
                summary.executed,
                summary.passed,
                summary.failed,
                summary.skipped
            ),
            (Some(2), Some(1), Some(1), Some(1))
        );
    }

    #[test]
    fn outside_example_error_before_any_test_is_build_failure() {
        let output = RunOutput {
            stdout: "0 examples, 0 failures, 1 error occurred outside of examples\n",
            stderr: "",
            exit_code: Some(1),
        };
        let summary = ADAPTER.parse(&output);
        assert_eq!(
            classify(&summary, output.exit_code),
            RunOutcome::BuildFailed
        );
    }
}
