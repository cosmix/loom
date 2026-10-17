//! The same commands must receive the same verdict on every assertion surface.
use crate::models::stage::{AcceptanceCriterion, CommandConfinement, Stage, TruthCheck};
use crate::plan::schema::{SuccessCriteria, WiringTest};
use crate::verify::criteria::assertions::{evaluate, AssertionSpec};
use crate::verify::criteria::{run_acceptance, CriterionResult};
use crate::verify::goal_backward::{verify_truth_checks, verify_wiring_tests};
use std::time::Duration;
use tempfile::TempDir;

fn check(command: &str, exit: i32) -> TruthCheck {
    TruthCheck {
        command: command.into(),
        exit_code: Some(exit),
        stdout_contains: vec![],
        stdout_not_contains: vec!["forbidden".into()],
        stderr_empty: Some(true),
        description: None,
    }
}

fn assert_surfaces(truth: TruthCheck, passed: bool) {
    let dir = TempDir::new().unwrap();
    let mut stage = Stage::new("shared-assertions".into(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Extended(truth.clone()));
    assert_eq!(
        run_acceptance(&stage, Some(dir.path()))
            .unwrap()
            .all_passed(),
        passed,
        "acceptance: {}",
        truth.command
    );
    let wiring = wiring(&truth);
    assert_eq!(
        verify_truth_checks(&[truth], dir.path(), None, CommandConfinement::default())
            .unwrap()
            .is_empty(),
        passed
    );
    assert_eq!(
        verify_wiring_tests(&[wiring], dir.path(), None, CommandConfinement::default())
            .unwrap()
            .is_empty(),
        passed
    );
}

fn wiring(truth: &TruthCheck) -> WiringTest {
    WiringTest {
        name: "shared assertion".into(),
        command: truth.command.clone(),
        description: None,
        success_criteria: SuccessCriteria {
            exit_code: truth.exit_code,
            stdout_contains: truth.stdout_contains.clone(),
            stdout_not_contains: truth.stdout_not_contains.clone(),
            stderr_contains: vec![],
            stderr_empty: truth.stderr_empty,
        },
    }
}

#[test]
fn assertion_surfaces_share_exit_and_output_semantics() {
    for (command, exit, passed) in [
        ("printf ok", 0, true),
        ("exit 7", 7, true),
        ("exit 1", 0, false),
        ("kill -TERM $$", -1, false),
        ("printf forbidden", 0, false),
        ("printf error >&2", 0, false),
    ] {
        assert_surfaces(check(command, exit), passed);
    }
}

#[test]
fn first_live_run_rejects_forbidden_suffix_beyond_capture_cap() {
    let command = "head -c 10485761 /dev/zero; printf forbidden";
    assert_surfaces(check(command, 0), false);
}

#[test]
fn typed_incomplete_capture_and_timeout_cannot_pass() {
    for (complete, timed_out, exit) in [
        (false, false, Some(0)),
        (true, true, Some(0)),
        (true, false, None),
    ] {
        let mut result = CriterionResult::new(
            "probe".into(),
            true,
            "".into(),
            "".into(),
            exit,
            Duration::ZERO,
            timed_out,
        );
        result.output_complete = complete;
        let evaluation = evaluate(&AssertionSpec::default(), &result);
        assert!(!evaluation.passed);
        assert!(!evaluation.failures.is_empty());
    }
}

#[test]
fn literal_capture_marker_is_ordinary_output() {
    assert_surfaces(check("printf '[output truncated at 10MB]'", 0), true);
}
