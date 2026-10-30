use crate::models::stage::StageStatus;

use super::create_test_stage;

#[test]
fn test_executing_can_transition_to_merge_conflict() {
    let status = StageStatus::Executing;
    assert!(status.can_transition_to(&StageStatus::MergeConflict));
}

#[test]
fn test_merge_conflict_can_transition_to_completed() {
    let status = StageStatus::MergeConflict;
    assert!(status.can_transition_to(&StageStatus::Completed));
}

#[test]
fn test_merge_conflict_can_transition_to_blocked() {
    let status = StageStatus::MergeConflict;
    assert!(status.can_transition_to(&StageStatus::Blocked));
}

#[test]
fn test_merge_conflict_cannot_transition_to_other_states() {
    let status = StageStatus::MergeConflict;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Queued));
    assert!(!status.can_transition_to(&StageStatus::Executing));
    assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
    assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
    assert!(!status.can_transition_to(&StageStatus::Skipped));
}

#[test]
fn test_stage_try_mark_merge_conflict_valid() {
    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_mark_merge_conflict();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(stage.merge_conflict); // merge_conflict flag should be set
}

#[test]
fn test_stage_try_mark_merge_conflict_invalid() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_mark_merge_conflict();
    assert!(result.is_err());
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert!(!stage.merge_conflict);
}

#[test]
fn test_stage_try_complete_merge_valid() {
    let mut stage = create_test_stage(StageStatus::MergeConflict);
    stage.merge_conflict = true;
    let result = stage.try_complete_merge();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(!stage.merge_conflict); // merge_conflict flag should be cleared
    assert!(stage.merged); // merged flag should be set
    assert!(stage.completed_at.is_some());
}

#[test]
fn test_stage_try_complete_merge_from_completed_keeps_completed_at() {
    // A failed auto-merge leaves a stage at Completed + !merged; a later
    // retry (`try_complete_merge` on an already-Completed stage) must not
    // reset `completed_at` to now.
    let mut stage = create_test_stage(StageStatus::Completed);
    stage.merged = false;
    let preset_completed_at = chrono::Utc::now() - chrono::Duration::hours(1);
    stage.completed_at = Some(preset_completed_at);

    let result = stage.try_complete_merge();

    assert!(result.is_ok());
    assert!(stage.merged);
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(stage.completed_at, Some(preset_completed_at));
}

#[test]
fn test_stage_try_complete_merge_invalid() {
    // try_complete_merge should fail from states that can't transition to Completed
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_complete_merge();
    assert!(result.is_err());
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert!(!stage.merged); // merged flag should not be set on failure
}

#[test]
fn test_merge_conflict_recovery_workflow() {
    // Scenario: Stage is executing, merge has conflicts, resolution succeeds
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> MergeConflict (merge detected conflicts)
    assert!(stage.try_mark_merge_conflict().is_ok());
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(stage.merge_conflict);

    // MergeConflict -> Completed (conflicts resolved)
    assert!(stage.try_complete_merge().is_ok());
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(!stage.merge_conflict);
    assert!(stage.merged);
}

#[test]
fn test_merge_conflict_failure_workflow() {
    // Scenario: Stage has merge conflicts, resolution fails
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> MergeConflict
    assert!(stage.try_mark_merge_conflict().is_ok());
    assert_eq!(stage.status, StageStatus::MergeConflict);

    // MergeConflict -> Blocked (resolution failed)
    assert!(stage.try_mark_blocked().is_ok());
    assert_eq!(stage.status, StageStatus::Blocked);
}

#[test]
fn test_merge_conflict_display() {
    assert_eq!(format!("{}", StageStatus::MergeConflict), "MergeConflict");
}

#[test]
fn test_same_status_transition_includes_merge_conflict() {
    let status = StageStatus::MergeConflict;
    assert!(status.can_transition_to(&status.clone()));
}

#[test]
fn a_conflict_is_handed_back_to_queued_with_the_refusal() {
    let mut stage = create_test_stage(StageStatus::Executing);
    stage.try_mark_merge_conflict().unwrap();
    stage.fix_attempts = 3;
    stage.stall_recoveries = 2;

    stage
        .try_hand_back_merge("tests failed".to_string())
        .unwrap();

    assert_eq!(stage.status, StageStatus::Queued);
    assert!(!stage.merge_conflict);
    assert!(stage.merge.block.is_none());
    assert_eq!(stage.merge.handbacks, 1);
    assert_eq!(
        stage.merge.handback_refusal.as_deref(),
        Some("tests failed")
    );
    assert_eq!(stage.fix_attempts, 0);
    assert_eq!(stage.stall_recoveries, 0);
}

#[test]
fn a_blocked_merge_is_handed_back_to_queued() {
    let mut stage = create_test_stage(StageStatus::MergeBlocked);
    stage.merge.handbacks = 1;

    stage.try_hand_back_merge("again".to_string()).unwrap();

    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.merge.handbacks, 2);
    assert_eq!(stage.merge.handback_refusal.as_deref(), Some("again"));
}

#[test]
fn a_stage_outside_its_merge_states_is_not_handed_back() {
    for status in [StageStatus::Executing, StageStatus::NeedsHumanReview] {
        let mut stage = create_test_stage(status.clone());

        let result = stage.try_hand_back_merge("nope".to_string());

        assert!(result.is_err(), "{status}");
        assert_eq!(stage.status, status);
        assert_eq!(stage.merge.handbacks, 0);
        assert!(stage.merge.handback_refusal.is_none());
    }
}

#[test]
fn the_handback_refusal_ends_with_a_completion_or_a_new_merge_state() {
    for target in [
        StageStatus::Completed,
        StageStatus::MergeConflict,
        StageStatus::MergeBlocked,
    ] {
        let mut stage = create_test_stage(StageStatus::Executing);
        stage.merge.handback_refusal = Some("tests failed".to_string());

        stage.try_transition(target.clone()).unwrap();

        assert!(stage.merge.handback_refusal.is_none(), "{target}");
    }
}

#[test]
fn the_handback_refusal_survives_a_hold_and_a_requeue() {
    let mut held = create_test_stage(StageStatus::Executing);
    held.merge.handback_refusal = Some("tests failed".to_string());
    held.try_transition(StageStatus::NeedsHumanReview).unwrap();
    assert_eq!(held.merge.handback_refusal.as_deref(), Some("tests failed"));

    held.try_transition(StageStatus::Queued).unwrap();
    assert_eq!(held.merge.handback_refusal.as_deref(), Some("tests failed"));
}
