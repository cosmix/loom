//! Minitest: Ruby test-file, Rake, and Rails invocations with summary counts.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::testrun::command::{distinct_files, regex_literal, shell_quote, shell_word};
use crate::testrun::recognize::{
    combined_output, command_args, has_flag, last_match, pattern, positionals,
};
use crate::testrun::{RunOutput, RunSummary, TestRunnerAdapter, TestTarget};

pub struct Minitest;

pub static ADAPTER: Minitest = Minitest;

/// Ruby options whose following word is not a test file.
const RUBY_VALUE_FLAGS: &[&str] = &["-I", "-r", "-e", "-C", "-E", "-S", "-n", "--name"];

/// Rails test options whose following word is not a test file.
const RAILS_VALUE_FLAGS: &[&str] = &[
    "-n",
    "--name",
    "-s",
    "--seed",
    "-e",
    "--exclude",
    "-j",
    "--jobs",
];

static RUNS: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^(\d+) runs?, \d+ assertions?, \d+ failures?, \d+ errors?, \d+ skips?\s*$")
});
static FAILURES: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^\d+ runs?, \d+ assertions?, (\d+) failures?, \d+ errors?, \d+ skips?\s*$")
});
static ERRORS: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^\d+ runs?, \d+ assertions?, \d+ failures?, (\d+) errors?, \d+ skips?\s*$")
});
static SKIPS: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^\d+ runs?, \d+ assertions?, \d+ failures?, \d+ errors?, (\d+) skips?\s*$")
});

fn is_test_file(file: &str) -> bool {
    file.rsplit('/').next().is_some_and(|name| {
        name == "test.rb"
            || name.ends_with("_test.rb")
            || (name.starts_with("test_") && name.ends_with(".rb"))
    })
}

/// A direct `ruby` invocation must load test/ and name a test file.
fn ruby_test_args(argv: &[String]) -> Option<Vec<String>> {
    let args = command_args(argv, &["ruby"])?;
    let test_load_path = args.iter().any(|word| word == "-Itest")
        || args
            .windows(2)
            .any(|pair| pair[0] == "-I" && pair[1] == "test");
    let test_file = positionals(&args, RUBY_VALUE_FLAGS)
        .iter()
        .any(|file| is_test_file(file));
    (test_load_path && test_file).then_some(args)
}

/// `TEST` and `TESTOPTS` narrow a Rake or Rails run even when they precede it.
fn has_test_assignment(argv: &[String]) -> bool {
    argv.iter()
        .any(|word| word.starts_with("TEST=") || word.starts_with("TESTOPTS="))
}

/// Minitest's short `-n` accepts an attached name as well as a separate one.
fn has_name_filter(args: &[String]) -> bool {
    has_flag(args, "-n")
        || has_flag(args, "--name")
        || args
            .iter()
            .any(|word| word.starts_with("-n") && !word.starts_with("--"))
}

impl TestRunnerAdapter for Minitest {
    fn name(&self) -> &'static str {
        "minitest"
    }

    fn language(&self) -> &'static str {
        "ruby"
    }

    fn recognizes(&self, argv: &[String]) -> bool {
        ruby_test_args(argv).is_some()
            || command_args(argv, &["rake", "test"]).is_some()
            || command_args(argv, &["rails", "test"]).is_some()
    }

    /// Rake and Rails run the suite only without a name, file, or assignment filter.
    fn is_full_run(&self, argv: &[String]) -> bool {
        if has_test_assignment(argv) {
            return false;
        }
        if let Some(args) = command_args(argv, &["rake", "test"]) {
            return !has_name_filter(&args) && positionals(&args, &[]).is_empty();
        }
        if let Some(args) = command_args(argv, &["rails", "test"]) {
            return !has_name_filter(&args) && positionals(&args, RAILS_VALUE_FLAGS).is_empty();
        }
        false
    }

    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        let file = shell_word(file);
        let name = shell_quote(&format!("/^{}$/", regex_literal(test)));
        format!("ruby -Itest {file} -n {name}")
    }

    /// Run each selected file once, including every test named in that file.
    fn select_command(&self, targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        let runs: Vec<String> = distinct_files(targets)
            .into_iter()
            .map(|file| format!("ruby -Itest {}", shell_word(file)))
            .collect();
        (!runs.is_empty()).then(|| runs.join(" && "))
    }

    /// The trailer counts runs, failures, errors and skips; no trailer means no count.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary {
        let text = combined_output(out);
        let Some(runs) = last_match(&RUNS, &text) else {
            return RunSummary::default();
        };
        let skipped = last_match(&SKIPS, &text);
        let executed = runs.saturating_sub(skipped.unwrap_or(0));
        let failures = last_match(&FAILURES, &text).unwrap_or(0);
        let errors = last_match(&ERRORS, &text).unwrap_or(0);
        let failed = failures.saturating_add(errors);
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
    fn exact_method_command_uses_minitest_name_filter() {
        let command =
            ADAPTER.single_test_command("test/model_test.rb", "test_alpha_passes", Path::new("."));
        assert_eq!(
            command,
            "ruby -Itest test/model_test.rb -n '/^test_alpha_passes$/'"
        );
    }

    #[test]
    fn hostile_test_name_stays_one_escaped_pattern() {
        let command = ADAPTER.single_test_command("a_test.rb", HOSTILE_NAME, Path::new("."));
        let expected = format!("/^{}$/", regex_literal(HOSTILE_NAME));
        assert_one_word(&command, &expected);
    }

    #[test]
    fn ruby_rake_and_rails_invocations_are_identified() {
        assert!(ADAPTER.recognizes(&words("ruby -Itest test/model_test.rb")));
        assert!(ADAPTER.recognizes(&words("ruby -I test test/model_test.rb")));
        assert!(ADAPTER.recognizes(&words("ruby -Itest test/test_model.rb")));
        assert!(ADAPTER.recognizes(&words("bundle exec rake test")));
        assert!(ADAPTER.recognizes(&words("bin/rails test")));
        assert!(ADAPTER.recognizes(&words("rails test")));
        assert!(!ADAPTER.recognizes(&words("ruby script.rb")));
        assert!(!ADAPTER.recognizes(&words("ruby -Itest script.rb")));
        assert!(!ADAPTER.recognizes(&words("rake spec")));
    }

    #[test]
    fn suite_recognition_respects_file_name_and_assignment_filters() {
        assert!(ADAPTER.is_full_run(&words("bundle exec rake test")));
        assert!(ADAPTER.is_full_run(&words("bin/rails test")));
        assert!(!ADAPTER.is_full_run(&words("rails test test/model_test.rb")));
        assert!(!ADAPTER.is_full_run(&words("rails test --name test_alpha_passes")));
        assert!(!ADAPTER.is_full_run(&words("ruby -Itest test/model_test.rb")));
        assert!(!ADAPTER.is_full_run(&words("TEST=test/model_test.rb rake test")));
        assert!(!ADAPTER.is_full_run(&words("bundle exec rake test TESTOPTS=-nfoo")));
    }

    #[test]
    fn selection_runs_every_distinct_file() {
        let targets = [
            TestTarget {
                file: "test/a_test.rb".to_string(),
                name: Some("test_one".to_string()),
            },
            TestTarget {
                file: "test/a_test.rb".to_string(),
                name: Some("test_two".to_string()),
            },
            TestTarget {
                file: "test/b_test.rb".to_string(),
                name: None,
            },
        ];
        assert_eq!(
            ADAPTER.select_command(&targets, Path::new(".")),
            Some("ruby -Itest test/a_test.rb && ruby -Itest test/b_test.rb".to_string())
        );
        assert_eq!(ADAPTER.select_command(&[], Path::new(".")), None);
    }

    #[test]
    fn recorded_minitest_trailers_produce_expected_verdicts() {
        let recorded = scenarios(ADAPTER.name());
        assert!(!recorded.is_empty(), "minitest fixtures are missing");
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
                _ => panic!("unknown minitest fixture: {scenario}"),
            };
            assert_eq!(classify(&summary, exit_code), expected, "{scenario}");
            if scenario.starts_with("suite") {
                assert_eq!((summary.executed, summary.failed), (Some(3), Some(1)));
            }
        }
    }
}
