//! A review round recorded after a completion's outside gate check is seen
//! by the gates the transition re-runs under the reviews lock.

use super::apply::{apply_finished, Applied};
use super::apply_binding::hooks::before_completion_lock;
use super::tests_complete_support::{record_review_round, Fixture, STAGE};
use super::verdict_complete::STAGE_BOUND_SESSION;
use super::{store, JobKind, JobState};
use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::update_stage;

/// Record a round with one open finding just before the completion locks.
fn arm_open_finding(fx: &Fixture) {
    let work_dir = fx.work_dir.clone();
    before_completion_lock(move || record_review_round(&work_dir, false, 1));
}

#[test]
#[serial_test::serial]
fn a_finding_recorded_after_the_outside_gate_check_rejects_the_completion() {
    let fx = Fixture::new(StageType::Standard, 2);
    fx.record_review(false);
    let record = fx.passed();
    arm_open_finding(&fx);

    let applied = fx.apply(&record).expect("apply the passed run");

    assert_eq!(applied, Applied::Recorded);
    fx.assert_rejected(&record);
}

#[test]
#[serial_test::serial]
fn a_finding_recorded_before_a_verdict_transition_hands_the_stage_back() {
    let fx = Fixture::new(StageType::Standard, 2);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.status = StageStatus::NeedsAdjudication;
        stage.session = None;
        Ok(())
    })
    .expect("hold the stage");
    fx.record_review(false);
    let mut record = fx.finished(
        JobKind::VerdictComplete,
        JobState::Passed,
        &[("acceptance-0", true)],
    );
    record.session_id = STAGE_BOUND_SESSION.to_string();
    store::save_current(&fx.work_dir, &record).expect("save the stage-bound record");
    arm_open_finding(&fx);

    let applied = apply_finished(&fx.work_dir, &record).expect("apply the passed run");

    assert!(matches!(applied, Applied::HandedBack(_)), "{applied:?}");
    assert_ne!(fx.load_stage().status, StageStatus::Completed);
}
