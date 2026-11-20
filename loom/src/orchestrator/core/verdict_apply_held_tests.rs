//! Applying a verdict to a stage the operator holds.

use super::*;

#[test]
fn a_held_plan_v1_stage_goes_to_a_fresh_session() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    assert_eq!(verdict_held_stage(&work, false), StageStatus::Queued);
}

#[test]
fn an_operator_held_stage_goes_back_to_the_queue_and_stays_held() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    assert_eq!(verdict_held_stage(&work, true), StageStatus::Queued);
    assert!(load_stage("test-stage", &work).unwrap().held);
}

#[test]
fn a_stage_with_an_unanswered_dispute_keeps_waiting() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    stage_needing_adjudication(&work);
    write_dispute_request(&work, "test-stage", 1);
    orchestrator_for(&work, temp.path())
        .apply_pending_verdicts()
        .unwrap();
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsAdjudication
    );
}
