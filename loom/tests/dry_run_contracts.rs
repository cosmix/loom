//! Contracts for the criteria-authoring stage: `loom plan verify --dry-run`.
//! The dry run tells an environment failure from a criterion that is red
//! before implementation, reruns a first timeout once, grades each finding by
//! the plan's shape, runs in a checkout of the committed HEAD, and provisions
//! that checkout before any criterion. A recording executor stands in for the
//! confined runner, so no criterion needs bubblewrap.

#[path = "criteria_authoring_contracts/plans.rs"]
mod plans;
#[path = "criteria_authoring_contracts/support.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use loom::commands::plan::verify::dry_run::{
    classify, run_dry_run_with, settle_timeout, severity, stage_cannot_tell, DryCommand,
    DryOutcome, DrySeverity,
};
use loom::commands::plan::verify::report::{VerifyError, VerifyReport};
use loom::verify::criteria::CriterionResult;
use plans::{provision, stage, standard, v1_plan, v2_plan};
use support::{committed_plan, exited, exited_stdout, passed, timed_out};

#[test]
fn dry_run_separates_environment_from_red() {
    let none: &[String] = &[];
    let missing = classify(&exited("x", 127, "sh: x: command not found"), none);
    assert!(matches!(missing, DryOutcome::Missing(_)), "{missing:?}");
    let denied = classify(
        &exited("./run.sh", 126, "sh: ./run.sh: Permission denied"),
        none,
    );
    assert!(matches!(denied, DryOutcome::Missing(_)), "{denied:?}");
    let environment = [
        exited(
            "cargo test",
            1,
            "bwrap: setting up uid map: Permission denied",
        ),
        exited(
            "curl x",
            1,
            "curl: (6) Temporary failure in name resolution",
        ),
        exited_stdout("fetch", 6, "Could not resolve host: registry.example"),
    ];
    for result in &environment {
        let outcome = classify(result, none);
        assert!(matches!(outcome, DryOutcome::Environment(_)), "{outcome:?}");
    }
    let red = exited("cargo test", 1, "assertion failed: left == right");
    assert_eq!(
        classify(&red, none),
        DryOutcome::Failed,
        "a red criterion is no error"
    );
    assert_eq!(
        classify(&timed_out("sleep 999"), none),
        DryOutcome::TimedOut
    );
    assert_eq!(classify(&passed("true"), none), DryOutcome::Passed);
    an_empty_selection_is_zero_tests();
}

/// The zero-test guard's failure, on a run whose exit code was 0.
fn an_empty_selection_is_zero_tests() {
    let selection = "cargo test --test new_tests new_case";
    let zero = CriterionResult::new(
        selection.to_string(),
        false,
        "running 0 tests".to_string(),
        String::new(),
        Some(0),
        Duration::from_millis(5),
        false,
    );
    let failures = vec![format!(
        "Command {selection}: selected zero tests (cargo-test)"
    )];
    assert_eq!(classify(&zero, &failures), DryOutcome::ZeroTests);
}

#[test]
fn a_first_timeout_is_run_again() {
    for second in [DryOutcome::Passed, DryOutcome::TimedOut] {
        let mut calls = 0;
        let settled = settle_timeout(DryOutcome::TimedOut, &mut || {
            calls += 1;
            second.clone()
        });
        assert_eq!(settled, second, "the rerun's outcome is the one reported");
        assert_eq!(calls, 1, "a first timeout runs exactly once more");
    }
    let others = [
        DryOutcome::Passed,
        DryOutcome::Failed,
        DryOutcome::Environment("bwrap:".to_string()),
    ];
    for first in others {
        let mut calls = 0;
        let settled = settle_timeout(first.clone(), &mut || {
            calls += 1;
            DryOutcome::TimedOut
        });
        assert_eq!(settled, first);
        assert_eq!(calls, 0, "{first:?} did not time out and is not rerun");
    }
}

fn errors_of(report: &VerifyReport, stage_id: &str) -> usize {
    let of_stage = |e: &&VerifyError| e.stage_id.as_deref() == Some(stage_id);
    report.errors.iter().filter(of_stage).count()
}

#[test]
fn a_second_timeout_is_a_timeout() {
    let stages = [
        standard("warms", &["slow-build"], ""),
        standard("hangs", &["hang-forever"], ""),
    ];
    let (_repo, path, mut report) = committed_plan(&v1_plan(&stages.concat()));
    let mut calls: Vec<(String, PathBuf)> = Vec::new();
    run_dry_run_with(&path, &mut report, &mut |command: &DryCommand| {
        calls.push((command.command.clone(), command.dir.clone()));
        let runs = calls.iter().filter(|(c, _)| *c == command.command).count();
        let result = match command.command.as_str() {
            "slow-build" if runs > 1 => passed("slow-build"),
            other => timed_out(other),
        };
        (result, Vec::new())
    })
    .expect("the dry run completes");
    for command in ["slow-build", "hang-forever"] {
        let dirs: Vec<&PathBuf> = calls
            .iter()
            .filter(|(c, _)| c == command)
            .map(|(_, d)| d)
            .collect();
        assert_eq!(
            dirs.len(),
            2,
            "{command} runs twice and never a third time: {calls:?}"
        );
        assert_eq!(
            dirs[0], dirs[1],
            "{command} reruns in the checkout it first ran in"
        );
    }
    assert_eq!(
        errors_of(&report, "warms"),
        0,
        "a rerun that passes is no timeout"
    );
    assert_eq!(
        errors_of(&report, "hangs"),
        1,
        "two timeouts are a timeout error"
    );
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
}

fn severity_table() {
    use DrySeverity::{Error, Note};
    let table = [
        (
            DryOutcome::Environment("bwrap:".to_string()),
            Some(Error),
            Some(Error),
        ),
        (DryOutcome::TimedOut, Some(Error), Some(Error)),
        (DryOutcome::ZeroTests, Some(Note), Some(Note)),
        (
            DryOutcome::Missing("command not found".to_string()),
            Some(Error),
            Some(Note),
        ),
        (DryOutcome::Passed, Some(Note), None),
        (DryOutcome::Failed, Some(Note), Some(Note)),
    ];
    for (outcome, free, dependent) in table {
        assert_eq!(
            severity(&outcome, false),
            free,
            "{outcome:?}, no dependencies"
        );
        assert_eq!(
            severity(&outcome, true),
            dependent,
            "{outcome:?}, with a dependency"
        );
    }
    let passing = [DryOutcome::Passed, DryOutcome::Passed];
    assert!(stage_cannot_tell(&passing, false));
    assert!(!stage_cannot_tell(&passing, true));
    assert!(!stage_cannot_tell(
        &[DryOutcome::Passed, DryOutcome::Failed],
        false
    ));
    assert!(!stage_cannot_tell(&[], false));
}

const ZERO: &str = "cargo test --test new_tests new_case";

fn severity_executor(command: &DryCommand) -> (CriterionResult, Vec<String>) {
    let text = command.command.as_str();
    match text {
        "true" | "loom knowledge check" => (passed(text), Vec::new()),
        "test -e absent" => (exited(text, 1, ""), Vec::new()),
        "missing-tool --version" => (exited(text, 127, "sh: missing-tool: not found"), Vec::new()),
        "bash scripts/new.sh" => {
            let stderr = "bash: scripts/new.sh: No such file or directory";
            (exited(text, 127, stderr), Vec::new())
        }
        ZERO => {
            let failure = format!("Command {ZERO}: selected zero tests (cargo-test)");
            (exited_stdout(text, 0, "running 0 tests"), vec![failure])
        }
        other => panic!("the plan has no command {other:?}"),
    }
}

/// The dry-run report of a plan holding `stages`, run with `severity_executor`.
fn severity_report(stages: &[String]) -> VerifyReport {
    let (_repo, path, mut report) = committed_plan(&v1_plan(&stages.concat()));
    run_dry_run_with(&path, &mut report, &mut |command: &DryCommand| {
        severity_executor(command)
    })
    .expect("the dry run completes");
    report
}

#[test]
fn dry_run_severity_follows_the_plan() {
    severity_table();
    let normal = vec![
        standard("mixed", &["true", "test -e absent"], ""),
        stage(
            "needs-tool",
            "standard",
            &["mixed"],
            &["missing-tool --version"],
            "",
        ),
        standard("zero-tests", &[ZERO], ""),
        standard(
            "new-script",
            &["bash scripts/new.sh"],
            "      files: [\"scripts/**\"]\n",
        ),
        stage("notes", "knowledge", &[], &["loom knowledge check"], ""),
    ];
    let mut all = vec![standard("only-passes", &["true"], "")];
    all.extend(normal.iter().cloned());
    let report = severity_report(&all);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let baseline = &report.warnings.baseline;
    assert_eq!(baseline.len(), 1, "one stage-level warning: {baseline:?}");
    assert!(baseline[0].contains("only-passes"), "{baseline:?}");
    for id in ["mixed", "needs-tool", "zero-tests", "new-script"] {
        assert!(
            report.notes.iter().any(|note| note.contains(id)),
            "{id}: {:?}",
            report.notes
        );
    }

    let report = severity_report(&normal);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(
        report.warnings.baseline.is_empty(),
        "{:?}",
        report.warnings.baseline
    );
}

/// Run `test [!] -e marker.txt` as the shell would in `command.dir`, which
/// must lie under `checkouts`; returns the canonical directory and the result.
fn marker_test(checkouts: &Path, command: &DryCommand) -> (PathBuf, CriterionResult) {
    let dir = command
        .dir
        .canonicalize()
        .expect("the checkout exists while it runs");
    assert!(
        dir.starts_with(checkouts),
        "{} is not under .loom/dry-run",
        dir.display()
    );
    let present = dir.join("marker.txt").exists();
    let success = match command.command.as_str() {
        "test ! -e marker.txt" => !present,
        "test -e marker.txt" => present,
        other => panic!("the plan has no command {other:?}"),
    };
    let result = if success {
        passed(&command.command)
    } else {
        exited(&command.command, 1, "")
    };
    (dir, result)
}

#[test]
fn dry_run_reads_the_committed_tree() {
    let stages = [
        standard("absent-at-base", &["test ! -e marker.txt"], ""),
        standard("needs-marker", &["test -e marker.txt"], ""),
    ];
    let (repo, path, mut report) = committed_plan(&v1_plan(&stages.concat()));
    repo.write("marker.txt", "untracked\n");
    let checkouts = repo.root.join(".loom").join("dry-run");
    let mut dirs = Vec::new();
    run_dry_run_with(&path, &mut report, &mut |command: &DryCommand| {
        let (dir, result) = marker_test(&checkouts, command);
        dirs.push(dir);
        (result, Vec::new())
    })
    .expect("the dry run completes");
    assert_eq!(dirs.len(), 2, "each criterion runs once");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let baseline = &report.warnings.baseline;
    assert_eq!(baseline.len(), 1, "{baseline:?}");
    assert!(
        baseline[0].contains("absent-at-base"),
        "HEAD has no marker.txt: {baseline:?}"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("needs-marker")),
        "{:?}",
        report.notes
    );
    assert!(
        dirs.iter().all(|dir| !dir.exists()),
        "the checkout is removed when the run ends"
    );
}

#[test]
fn dry_run_provisions_before_the_criteria() {
    let (_repo, path, mut report) =
        committed_plan(&v2_plan(&provision("touch provisioned.txt"), ""));
    let mut seen = Vec::new();
    run_dry_run_with(&path, &mut report, &mut |command: &DryCommand| {
        let provisioned = command.dir.join("provisioned.txt").exists();
        seen.push(provisioned);
        let result = if provisioned {
            passed(&command.command)
        } else {
            exited(&command.command, 127, "")
        };
        (result, Vec::new())
    })
    .expect("the dry run completes");
    assert!(!seen.is_empty(), "the criteria ran");
    assert!(
        seen.iter().all(|provisioned| *provisioned),
        "provision runs first: {seen:?}"
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);

    let (_repo, path, mut report) = committed_plan(&v2_plan(&provision("exit 3"), ""));
    let mut calls = 0;
    run_dry_run_with(&path, &mut report, &mut |command: &DryCommand| {
        calls += 1;
        (passed(&command.command), Vec::new())
    })
    .expect("a failed provision is a finding");
    assert_eq!(calls, 0, "no criterion runs after a failed provision");
    let named = report
        .errors
        .iter()
        .any(|e| e.stage_id.is_none() && e.message.contains("provision"));
    assert!(named, "an error names the provision: {:?}", report.errors);
}
