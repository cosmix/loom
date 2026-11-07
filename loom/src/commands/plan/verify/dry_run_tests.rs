//! Tests for the dry run's classification and its flow over a committed
//! plan. A recording executor stands in for the confined runner, so only the
//! provision cases need bubblewrap. The real executor's tests are in
//! `dry_run_real_tests.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tempfile::TempDir;

use super::*;
use crate::commands::plan::verify::report::verify_text;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};

pub(super) fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// One stage entry of a plan; `extra` holds further keys (six-space indented lines).
pub(super) fn stage(
    id: &str,
    kind: &str,
    deps: &[&str],
    acceptance: &[&str],
    extra: &str,
) -> String {
    let deps: Vec<String> = deps.iter().map(|dep| format!("\"{dep}\"")).collect();
    let acceptance: String = acceptance
        .iter()
        .map(|c| format!("        - \"{c}\"\n"))
        .collect();
    format!(
        "    - id: {id}\n      name: \"{id}\"\n      stage_type: {kind}\n      working_dir: \".\"\n      \
         dependencies: [{}]\n      summary: \"Stage {id}.\"\n      description: \"Stage {id}.\"\n      \
         acceptance:\n{acceptance}{extra}",
        deps.join(", ")
    )
}

/// A version 1 plan with `loom_extra` (two-space keys) under `loom:`.
pub(super) fn plan(loom_extra: &str, stages: &[String]) -> String {
    format!(
        "# PLAN: Dry Run Fixture\n\n---\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n\
         {loom_extra}  stages:\n{}```\n\n<!-- END loom METADATA -->\n",
        stages.concat()
    )
}

/// A repository with `text` committed as `PLAN.md`: its directory and the plan's path.
pub(super) fn commit_plan(text: &str) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.com"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    std::fs::write(root.join("PLAN.md"), text).unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "plan"]);
    (tmp, root.join("PLAN.md"))
}

/// [`commit_plan`] and the plan's `verify_text` report, which holds no error.
pub(super) fn committed(text: &str) -> (TempDir, PathBuf, VerifyReport) {
    let (tmp, path) = commit_plan(text);
    let report = verify_text(None, &path, text);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    (tmp, path, report)
}

pub(super) fn result(
    command: &str,
    exit: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> CriterionResult {
    let (out, err) = (stdout.to_string(), stderr.to_string());
    let success = exit == Some(0);
    CriterionResult::new(
        command.to_string(),
        success,
        out,
        err,
        exit,
        Duration::ZERO,
        false,
    )
}

fn timed_out(command: &str) -> CriterionResult {
    let none = String::new;
    CriterionResult::new(
        command.to_string(),
        false,
        none(),
        none(),
        None,
        Duration::ZERO,
        true,
    )
}

#[test]
fn classify_separates_environment_from_red() {
    let none: &[String] = &[];
    let missing = classify(
        &result("x", Some(127), "", "sh: x: command not found"),
        none,
    );
    assert!(matches!(missing, DryOutcome::Missing(_)), "{missing:?}");
    let red = result("cargo test", Some(1), "", "assertion failed");
    assert_eq!(classify(&red, none), DryOutcome::Failed);
    let denied = result(
        "cargo test",
        Some(101),
        "test result: FAILED",
        "Operation not permitted",
    );
    assert!(matches!(
        classify(&denied, none),
        DryOutcome::Environment(_)
    ));
    assert_eq!(classify_with(&denied, none, true), DryOutcome::Failed);
    let full = result("w", Some(1), "", "No space left on device");
    assert!(matches!(classify(&full, none), DryOutcome::Environment(_)));
    assert_eq!(classify(&timed_out("sleep 9"), none), DryOutcome::TimedOut);
    assert_eq!(
        classify(&result("true", Some(0), "", ""), none),
        DryOutcome::Passed
    );
    let zero = result("cargo test x", Some(0), "running 0 tests", "");
    let failures = vec!["Command cargo test x: selected zero tests (cargo-test)".to_string()];
    assert_eq!(classify(&zero, &failures), DryOutcome::ZeroTests);
}

#[test]
fn settle_timeout_reruns_only_a_timeout() {
    for second in [DryOutcome::Passed, DryOutcome::TimedOut] {
        let mut calls = 0;
        let settled = settle_timeout(DryOutcome::TimedOut, &mut || {
            calls += 1;
            second.clone()
        });
        assert_eq!((settled, calls), (second, 1));
    }
    for first in [
        DryOutcome::Passed,
        DryOutcome::Failed,
        DryOutcome::Environment("x".into()),
    ] {
        let settled = settle_timeout(first.clone(), &mut || panic!("{first:?} is not rerun"));
        assert_eq!(settled, first);
    }
}

#[test]
fn severity_follows_dependencies() {
    use DrySeverity::{Error, Note};
    let table = [
        (
            DryOutcome::Environment("bwrap:".into()),
            Some(Error),
            Some(Error),
        ),
        (DryOutcome::TimedOut, Some(Error), Some(Error)),
        (DryOutcome::ZeroTests, Some(Note), Some(Note)),
        (
            DryOutcome::Missing("exit 127".into()),
            Some(Error),
            Some(Note),
        ),
        (DryOutcome::Passed, Some(Note), None),
        (DryOutcome::Failed, Some(Note), Some(Note)),
    ];
    for (outcome, free, dependent) in table {
        assert_eq!(severity(&outcome, false), free, "{outcome:?}");
        assert_eq!(severity(&outcome, true), dependent, "{outcome:?}");
    }
    let passed = [DryOutcome::Passed, DryOutcome::Passed];
    assert!(stage_cannot_tell(&passed, false));
    assert!(!stage_cannot_tell(&passed, true));
    assert!(!stage_cannot_tell(
        &[DryOutcome::Passed, DryOutcome::Failed],
        false
    ));
    assert!(!stage_cannot_tell(&[], false));
}

/// Runs `run_dry_run_with` over `text` with `answer` standing in for the runner.
fn dry_report(text: &str, answer: &mut dyn FnMut(&DryCommand) -> CriterionResult) -> VerifyReport {
    let (_tmp, path, mut report) = committed(text);
    let mut execute = |command: &DryCommand| (answer(command), Vec::new());
    run_dry_run_with(&path, &mut report, &mut execute).unwrap();
    report
}

#[test]
fn a_second_timeout_is_an_error_and_a_pass_on_the_rerun_is_not() {
    let stages = [
        stage("warms", "standard", &[], &["slow-build"], ""),
        stage("hangs", "standard", &[], &["hang"], ""),
    ];
    let mut calls = Vec::new();
    let report = dry_report(&plan("", &stages), &mut |command: &DryCommand| {
        calls.push(command.command.clone());
        let reruns = calls.iter().filter(|c| **c == command.command).count();
        if command.command == "slow-build" && reruns > 1 {
            return result("slow-build", Some(0), "", "");
        }
        timed_out(&command.command)
    });
    assert_eq!(calls.iter().filter(|c| *c == "hang").count(), 2);
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert_eq!(report.errors[0].stage_id.as_deref(), Some("hangs"));
    assert!(report.errors[0].message.contains("second run"));
}

#[test]
fn a_missing_program_is_an_error_only_where_nothing_can_provide_it() {
    let stages = [
        stage("free", "standard", &[], &["tool-a"], ""),
        stage("later", "standard", &["free"], &["tool-b"], ""),
        stage(
            "creates",
            "standard",
            &[],
            &["bash scripts/new.sh"],
            "      files: [\"scripts/**\"]\n",
        ),
    ];
    let report = dry_report(&plan("", &stages), &mut |command: &DryCommand| {
        result(&command.command, Some(127), "", "not found")
    });
    let failing: Vec<_> = report
        .errors
        .iter()
        .map(|e| e.stage_id.clone().unwrap())
        .collect();
    assert_eq!(failing, ["free"], "{:?}", report.errors);
    for id in ["later", "creates"] {
        assert!(
            report.notes.iter().any(|n| n.contains(id)),
            "{id}: {:?}",
            report.notes
        );
    }
    assert!(report
        .notes
        .iter()
        .any(|n| n.contains("cover `scripts/new.sh`")));
}

#[test]
fn only_a_standard_stage_that_passes_everything_cannot_tell() {
    let stages = [
        stage("flat", "standard", &[], &["true"], ""),
        stage("mixed", "standard", &[], &["true", "false"], ""),
        stage("notes", "knowledge", &[], &["loom knowledge check"], ""),
    ];
    let report = dry_report(&plan("", &stages), &mut |command: &DryCommand| {
        let exit = i32::from(command.command == "false");
        result(&command.command, Some(exit), "", "")
    });
    let baseline = &report.warnings.baseline;
    assert_eq!(baseline.len(), 1, "{baseline:?}");
    assert!(baseline[0].contains("`flat`"));
    assert!(report.errors.is_empty());
}

#[test]
fn the_executor_sees_one_committed_checkout_that_is_removed_afterwards() {
    let stages = [stage("a", "standard", &[], &["test -e marker.txt"], "")];
    let (tmp, path, mut report) = committed(&plan("", &stages));
    std::fs::write(tmp.path().join("marker.txt"), "untracked").unwrap();
    let parent = tmp.path().canonicalize().unwrap().join(".loom/dry-run");
    let mut dirs = Vec::new();
    run_dry_run_with(&path, &mut report, &mut |command: &DryCommand| {
        assert!(
            command.dir.starts_with(&parent),
            "{}",
            command.dir.display()
        );
        assert!(!command.dir.join("marker.txt").exists());
        dirs.push(command.dir.clone());
        (result(&command.command, Some(1), "", ""), Vec::new())
    })
    .unwrap();
    assert_eq!(dirs.len(), 1);
    assert!(
        !dirs[0].exists(),
        "the checkout is removed when the run ends"
    );
}

#[test]
fn provision_runs_before_the_criteria_and_a_failure_stops_the_run() {
    let reason = "bubblewrap runs the provision entries";
    if skip_unless(
        confined_commands_runnable(),
        "provision_runs_before_the_criteria",
        reason,
    ) {
        return;
    }
    // Provision needs a version 2 plan to validate; the dry run does not check that.
    let entry = |command: &str| {
        format!("  provision:\n    - working_dir: \".\"\n      command: \"{command}\"\n")
    };
    let stages = [stage("a", "standard", &[], &["true"], "")];
    let dry = |command: &str, answer: &mut dyn FnMut(&DryCommand) -> CriterionResult| {
        let text = plan(&entry(command), &stages);
        let (_tmp, path) = commit_plan(&text);
        let mut report = verify_text(None, &path, &text);
        let mut execute = |c: &DryCommand| (answer(c), Vec::new());
        run_dry_run_with(&path, &mut report, &mut execute).unwrap();
        report.errors.retain(|e| e.message.starts_with("dry run"));
        report
    };
    let report = dry("touch provisioned.txt", &mut |command: &DryCommand| {
        let ready = command.dir.join("provisioned.txt").exists();
        result(&command.command, Some(if ready { 0 } else { 127 }), "", "")
    });
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let mut calls = 0;
    let report = dry("exit 3", &mut |command: &DryCommand| {
        calls += 1;
        result(&command.command, Some(0), "", "")
    });
    assert_eq!(calls, 0);
    assert!(report
        .errors
        .iter()
        .any(|e| e.stage_id.is_none() && e.message.contains("provision failed")));
}

#[test]
fn the_confinement_refusal_names_the_probe_reason_and_its_remedy() {
    use crate::commands::plan::verify::dry_run_exec::needs_confinement;

    let missing = needs_confinement("bwrap not found");
    assert!(missing.contains("needs a shell where loom can confine commands (bwrap not found)"));
    assert!(missing.contains("install bubblewrap"), "{missing}");
    assert!(!missing.contains("operator's shell"), "{missing}");

    let denied = needs_confinement("bwrap failed (exit status: 1: setting up uid map)");
    assert!(denied.contains("setting up uid map"), "{denied}");
    assert!(
        denied.contains("run it from the operator's shell"),
        "{denied}"
    );
    assert!(!denied.contains("install bubblewrap"), "{denied}");
}
