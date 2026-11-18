//! The real, confined executor of `loom plan verify --dry-run`, end to end.
//! Each test needs bubblewrap and skips where a session's own sandbox refuses it.

use super::dry_run_tests::{committed, plan, stage};
use super::*;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};

fn skipped(test: &str) -> bool {
    let why = "the real executor confines every command with bubblewrap";
    skip_unless(confined_commands_runnable(), test, why)
}

/// Runs the real dry run over `text` and returns the report.
fn dry_run_of(text: &str) -> VerifyReport {
    let (_tmp, path, mut report) = committed(text);
    run_dry_run(&path, &mut report).unwrap();
    report
}

#[test]
fn a_sound_plan_leaves_no_error_and_no_warning() {
    if skipped("a_sound_plan_leaves_no_error_and_no_warning") {
        return;
    }
    let stages = [
        stage("first", "standard", &[], &["true", "test -e absent"], ""),
        stage("second", "standard", &["first"], &["true"], ""),
    ];
    let text = plan("", &stages);
    let (_tmp, path, mut report) = committed(&text);
    let before = report.warnings.total();
    run_dry_run(&path, &mut report).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(
        report.warnings.baseline.is_empty(),
        "{:?}",
        report.warnings.baseline
    );
    assert_eq!(report.warnings.total(), before);
}

#[test]
fn the_checkout_holds_committed_files_only() {
    if skipped("the_checkout_holds_committed_files_only") {
        return;
    }
    let stages = [
        stage("needs", "standard", &[], &["test -f marker.txt"], ""),
        stage("absent", "standard", &[], &["test ! -e marker.txt"], ""),
    ];
    let (tmp, path, mut report) = committed(&plan("", &stages));
    std::fs::write(tmp.path().join("marker.txt"), "untracked").unwrap();
    run_dry_run(&path, &mut report).unwrap();
    assert!(report
        .notes
        .iter()
        .any(|n| n.contains("needs") && n.contains("fails at the base")));
    let baseline = &report.warnings.baseline;
    assert!(
        baseline.len() == 1 && baseline[0].contains("`absent`"),
        "{baseline:?}"
    );
}

#[test]
fn a_command_slow_with_a_warm_cache_is_a_timeout() {
    if skipped("a_command_slow_with_a_warm_cache_is_a_timeout") {
        return;
    }
    let slow = "        - command: \"sleep 4\"\n          timeout_secs: 2\n";
    let text = plan("", &[stage("slow", "standard", &[], &[], "")])
        .replace("      acceptance:\n", &format!("      acceptance:\n{slow}"));
    let report = dry_run_of(&text);
    let error = report.errors.first().expect("a timeout error");
    assert!(error.message.contains("second run"), "{}", error.message);
}

#[test]
fn the_plans_inherit_confinement_reaches_acceptance() {
    if skipped("the_plans_inherit_confinement_reaches_acceptance") {
        return;
    }
    let _guard = EnvGuard::set("LOOM_DRY_RUN_PROBE", "1");
    let sandbox = "  sandbox:\n    command_confinement: inherit\n";
    let stages = [stage(
        "env",
        "standard",
        &[],
        &["test \\\"$LOOM_DRY_RUN_PROBE\\\" = 1"],
        "",
    )];
    let report = dry_run_of(&plan(sandbox, &stages));
    let notes = &report.notes;
    assert!(
        notes.iter().any(|n| n.contains("already passes")),
        "{notes:?}"
    );
}

#[test]
fn after_stage_checks_are_judged_as_completion_judges_them() {
    if skipped("after_stage_checks_are_judged_as_completion_judges_them") {
        return;
    }
    let checks =
        "      after_stage:\n        - command: \"false\"\n          exit_code: 1\n        - \
                  command: \"echo hi\"\n          stdout_contains: [\"absent\"]\n";
    let report = dry_run_of(&plan(
        "",
        &[stage("gate", "standard", &[], &["true"], checks)],
    ));
    let notes = &report.notes;
    assert!(
        notes
            .iter()
            .any(|n| n.contains("after_stage 0") && n.contains("already passes")),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("after_stage 1") && n.contains("fails at the base")),
        "{notes:?}"
    );
}

/// Sets an environment variable for one test and restores it.
struct EnvGuard(&'static str, Option<String>);

impl EnvGuard {
    fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var(name).ok();
        std::env::set_var(name, value);
        Self(name, previous)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.1 {
            Some(value) => std::env::set_var(self.0, value),
            None => std::env::remove_var(self.0),
        }
    }
}
