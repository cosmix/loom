//! Contract `sandbox-denial-is-environment`: captured runner output that a
//! refused sandbox produced is named by the shared classifier
//! (`environment::sandbox_denial`), shown on a failed acceptance criterion
//! that keeps its verdict and original index, and turned into a note by the
//! impact judge. A counted assertion that mentions `EPERM`, a real build
//! failure and a passing run keep their own verdicts. Assertion messages are
//! fixed text: the fixtures hold the very markers the freeze treats as an
//! environment failure.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use tempfile::TempDir;

use crate::testrun::{registry, TestTarget};
use crate::verify::criteria::{format_criterion_result, CriterionResult, ProbeRun, ProbeRunner};
use crate::verify::environment::sandbox_denial;
use crate::verify::impact_tests::Selection;

const FULL_SUITE: &str = "the full suite runs in integration-verify";
const LAUNCHER_DENIAL: &str = "sandbox-exec: sandbox_apply: Operation not permitted\n";
const BUN_DENIAL: &str = "bun test v1.2.0 (b0c1a2d3)\nerror: CouldntReadCurrentDirectory\n";
const NODE_KILL: &str = "node:internal/child_process:511\n    Error: kill EPERM\n    at kill\n";
const BWRAP: &str = "bwrap: Creating new namespace failed: Operation not permitted\n";
const COUNTED_EPERM: &str = "running 1 test\ntest signals::kills_child ... FAILED\n\n\
    failures:\n\n---- signals::kills_child stdout ----\n\
    thread 'signals::kills_child' panicked at src/signals.rs:9:5:\n\
    assertion `left == right` failed: kill returned EPERM: operation not permitted\n\
    \x20 left: \"EPERM\"\n right: \"ESRCH\"\n\nfailures:\n    signals::kills_child\n\n\
    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; \
    finished in 0.00s\n";
const BUILD_FAILURE: &str = "error[E0425]: cannot find value `EPERM` in this scope\n \
    --> src/lib.rs:3:13\n\nerror: could not compile `demo` (lib test) due to 1 previous error\n";
const PASSING: &str = "running 1 test\ntest signals::kills_child ... ok\n\n\
    test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
    finished in 0.00s\n";

/// Answers every run with the same captured output.
struct Captured {
    stdout: &'static str,
    stderr: &'static str,
    exit_code: Option<i32>,
}

impl ProbeRunner for Captured {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        Ok(ProbeRun {
            stdout: self.stdout.to_string(),
            stderr: self.stderr.to_string(),
            exit_code: self.exit_code,
            timed_out: false,
            output_complete: true,
            cached: None,
        })
    }
}

fn criterion(stdout: &str, stderr: &str, exit_code: Option<i32>) -> CriterionResult {
    let (command, success) = ("cargo test".to_string(), exit_code == Some(0));
    let (stdout, stderr) = (stdout.to_string(), stderr.to_string());
    CriterionResult::new(
        command,
        success,
        stdout,
        stderr,
        exit_code,
        Duration::ZERO,
        false,
    )
}

fn the_classifier_names_the_first_denial_line() {
    for (output, line) in [
        (
            LAUNCHER_DENIAL,
            "sandbox-exec: sandbox_apply: Operation not permitted",
        ),
        (BUN_DENIAL, "error: CouldntReadCurrentDirectory"),
        (NODE_KILL, "Error: kill EPERM"),
        (
            BWRAP,
            "bwrap: Creating new namespace failed: Operation not permitted",
        ),
    ] {
        let named = sandbox_denial(output);
        assert!(
            named.as_deref() == Some(line),
            "a denial is named by its first diagnostic line, trimmed"
        );
    }
    let long = format!("Error: kill EPERM {}", "é".repeat(400));
    let named = sandbox_denial(&long).unwrap_or_default();
    assert!(
        named.starts_with("Error: kill EPERM") && named.len() <= 300 && long.starts_with(&named),
        "a long denial line is cut to 300 bytes on a character boundary"
    );
    assert!(
        sandbox_denial(PASSING).is_none(),
        "a passing run names no denial"
    );
}

fn acceptance_shows_the_denial_on_a_failed_criterion() {
    let shown = format_criterion_result(7, &criterion("", BUN_DENIAL, Some(1)), &[]);
    assert!(
        shown.contains("FAILED [criterion 7]"),
        "a denied criterion still fails under its original index"
    );
    let denial = shown
        .lines()
        .find(|line| line.to_lowercase().contains("sandbox denial"));
    assert!(
        denial.is_some_and(|line| line.contains("CouldntReadCurrentDirectory")),
        "the failure names the denial line"
    );
    assert!(
        shown.contains("loom stage dispute-criteria") && shown.contains("--criterion-index 7"),
        "the dispute hint names the criterion's original index"
    );
    let counted = format_criterion_result(2, &criterion(COUNTED_EPERM, "", Some(101)), &[]);
    assert!(
        counted.contains("FAILED [criterion 2]"),
        "a counted assertion failure stays a failure"
    );
    let passed = format_criterion_result(3, &criterion(PASSING, "", Some(0)), &[]);
    assert!(
        passed.lines().count() == 1 && !passed.to_lowercase().contains("sandbox denial"),
        "a passing criterion renders one line and names no denial"
    );
}

/// The selection's passed runs, notes and failures after one cargo run.
fn judged(run: Captured) -> (usize, Vec<String>, usize) {
    let root = TempDir::new().unwrap();
    let targets = [TestTarget {
        file: "src/lib.rs".to_string(),
        name: Some("signals::kills_child".to_string()),
    }];
    let mut selection = Selection::new(root.path(), &run, BTreeSet::new());
    selection.run_group(
        Path::new(""),
        registry::by_name("cargo-test").unwrap(),
        &targets,
    );
    let notes = selection.notes.iter().cloned().collect();
    (selection.ran.len(), notes, selection.failed.len())
}

fn the_impact_judge_notes_only_a_denied_selection() {
    let (ran, notes, failed) = judged(Captured {
        stdout: "",
        stderr: LAUNCHER_DENIAL,
        exit_code: Some(71),
    });
    assert!(
        ran == 0 && failed == 0,
        "a denied selection is no impact-test failure"
    );
    let note = notes.first().map(String::as_str).unwrap_or_default();
    assert!(
        notes.len() == 1 && note.contains("sandbox_apply") && note.contains("environment"),
        "a denied selection is one environment note naming the denial"
    );
    assert!(
        note.matches(FULL_SUITE).count() == 1,
        "the note gives the full-suite hint once"
    );
    let counted = judged(Captured {
        stdout: COUNTED_EPERM,
        stderr: "",
        exit_code: Some(101),
    });
    assert!(
        counted.1.is_empty() && counted.2 == 1,
        "a counted assertion that mentions EPERM stays a failure"
    );
    let build = judged(Captured {
        stdout: "",
        stderr: BUILD_FAILURE,
        exit_code: Some(101),
    });
    assert!(
        build.1.is_empty() && build.2 == 1,
        "a real build failure stays a failure"
    );
    let passing = judged(Captured {
        stdout: PASSING,
        stderr: "",
        exit_code: Some(0),
    });
    assert!(
        passing.0 == 1 && passing.1.is_empty() && passing.2 == 0,
        "a passing selection passes with no note"
    );
}

#[test]
fn sandbox_denials_are_named_and_assertion_failures_are_not() {
    the_classifier_names_the_first_denial_line();
    acceptance_shows_the_denial_on_a_failed_criterion();
    the_impact_judge_notes_only_a_denied_selection();
}
