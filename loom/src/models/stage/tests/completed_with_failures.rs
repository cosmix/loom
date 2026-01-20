use crate::models::stage::StageStatus;

use super::create_test_stage;

#[test]
fn test_executing_can_transition_to_completed_with_failures() {
    let status = StageStatus::Executing;
    assert!(status.can_transition_to(&StageStatus::CompletedWithFailures));
}

#[test]
fn test_completed_with_failures_can_transition_to_executing() {
    let status = StageStatus::CompletedWithFailures;
    assert!(status.can_transition_to(&StageStatus::Executing));
}

#[test]
fn test_completed_with_failures_can_transition_to_queued() {
    let status = StageStatus::CompletedWithFailures;
    assert!(status.can_transition_to(&StageStatus::Queued));
}

#[test]
fn test_completed_with_failures_cannot_transition_to_invalid_states() {
    let status = StageStatus::CompletedWithFailures;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    // Note: Completed IS valid now (for re-verify)
    assert!(status.can_transition_to(&StageStatus::Completed));
    assert!(!status.can_transition_to(&StageStatus::Blocked));
    assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
    assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
    assert!(!status.can_transition_to(&StageStatus::Skipped));
    assert!(!status.can_transition_to(&StageStatus::MergeConflict));
    assert!(!status.can_transition_to(&StageStatus::MergeBlocked));
}

#[test]
fn test_stage_try_complete_with_failures_valid() {
    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_complete_with_failures();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::CompletedWithFailures);
}

#[test]
fn test_stage_try_complete_with_failures_invalid() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_complete_with_failures();
    assert!(result.is_err());
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_completed_with_failures_retry_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> CompletedWithFailures (acceptance criteria failed)
    assert!(stage.try_complete_with_failures().is_ok());
    assert_eq!(stage.status, StageStatus::CompletedWithFailures);

    // CompletedWithFailures -> Executing (retry)
    assert!(stage.try_mark_executing().is_ok());
    assert_eq!(stage.status, StageStatus::Executing);

    // Executing -> Completed (retry succeeds)
    assert!(stage.try_complete(None).is_ok());
    assert_eq!(stage.status, StageStatus::Completed);
}

#[test]
fn test_completed_with_failures_display() {
    assert_eq!(
        format!("{}", StageStatus::CompletedWithFailures),
        "CompletedWithFailures"
    );
}

#[test]
fn test_same_status_transition_includes_completed_with_failures() {
    let status = StageStatus::CompletedWithFailures;
    assert!(status.can_transition_to(&status.clone()));
}
