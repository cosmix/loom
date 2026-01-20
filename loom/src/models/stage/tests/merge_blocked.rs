use crate::models::stage::StageStatus;

use super::create_test_stage;

#[test]
fn test_executing_can_transition_to_merge_blocked() {
    let status = StageStatus::Executing;
    assert!(status.can_transition_to(&StageStatus::MergeBlocked));
}

#[test]
fn test_merge_blocked_can_transition_to_executing() {
    let status = StageStatus::MergeBlocked;
    assert!(status.can_transition_to(&StageStatus::Executing));
}

#[test]
fn test_merge_blocked_can_transition_to_queued() {
    let status = StageStatus::MergeBlocked;
    assert!(status.can_transition_to(&StageStatus::Queued));
}

#[test]
fn test_merge_blocked_cannot_transition_to_other_states() {
    let status = StageStatus::MergeBlocked;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Completed));
    assert!(!status.can_transition_to(&StageStatus::Blocked));
    assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
    assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
    assert!(!status.can_transition_to(&StageStatus::Skipped));
    assert!(!status.can_transition_to(&StageStatus::MergeConflict));
    assert!(!status.can_transition_to(&StageStatus::CompletedWithFailures));
}

#[test]
fn test_stage_try_mark_merge_blocked_valid() {
    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_mark_merge_blocked();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::MergeBlocked);
}

#[test]
fn test_stage_try_mark_merge_blocked_invalid() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_mark_merge_blocked();
    assert!(result.is_err());
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_merge_blocked_retry_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> MergeBlocked (merge failed with error)
    assert!(stage.try_mark_merge_blocked().is_ok());
    assert_eq!(stage.status, StageStatus::MergeBlocked);

    // MergeBlocked -> Executing (retry)
    assert!(stage.try_mark_executing().is_ok());
    assert_eq!(stage.status, StageStatus::Executing);

    // Executing -> Completed (retry succeeds)
    assert!(stage.try_complete(None).is_ok());
    assert_eq!(stage.status, StageStatus::Completed);
}

#[test]
fn test_merge_blocked_display() {
    assert_eq!(format!("{}", StageStatus::MergeBlocked), "MergeBlocked");
}

#[test]
fn test_same_status_transition_includes_merge_blocked() {
    let status = StageStatus::MergeBlocked;
    assert!(status.can_transition_to(&status.clone()));
}
