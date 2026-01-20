use crate::models::stage::StageStatus;

use super::create_test_stage;

#[test]
fn test_skipped_is_terminal_state() {
    let status = StageStatus::Skipped;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Queued));
    assert!(!status.can_transition_to(&StageStatus::Executing));
    assert!(!status.can_transition_to(&StageStatus::Blocked));
    assert!(!status.can_transition_to(&StageStatus::Completed));
    assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
    assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
}

#[test]
fn test_blocked_can_transition_to_skipped() {
    let status = StageStatus::Blocked;
    assert!(status.can_transition_to(&StageStatus::Skipped));

    let mut stage = create_test_stage(StageStatus::Blocked);
    let result = stage.try_skip(Some("User requested skip".to_string()));
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Skipped);
    assert_eq!(stage.close_reason, Some("User requested skip".to_string()));
}

#[test]
fn test_waiting_for_deps_can_transition_to_skipped() {
    let status = StageStatus::WaitingForDeps;
    assert!(status.can_transition_to(&StageStatus::Skipped));

    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_skip(Some("Not needed".to_string()));
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Skipped);
    assert_eq!(stage.close_reason, Some("Not needed".to_string()));
}

#[test]
fn test_queued_can_transition_to_skipped() {
    let status = StageStatus::Queued;
    assert!(status.can_transition_to(&StageStatus::Skipped));

    let mut stage = create_test_stage(StageStatus::Queued);
    let result = stage.try_skip(None);
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Skipped);
    assert_eq!(stage.close_reason, None);
}

#[test]
fn test_stage_try_skip_valid() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_skip(Some("Skipped by user".to_string()));
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Skipped);
    assert_eq!(stage.close_reason, Some("Skipped by user".to_string()));

    // Verify it's terminal - can't transition from Skipped
    let result = stage.try_mark_queued();
    assert!(result.is_err());
}

#[test]
fn test_executing_cannot_transition_to_skipped() {
    let status = StageStatus::Executing;
    assert!(!status.can_transition_to(&StageStatus::Skipped));

    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_skip(Some("Cannot skip".to_string()));
    assert!(result.is_err());
    assert_eq!(stage.status, StageStatus::Executing); // Status unchanged
}

#[test]
fn test_skipped_display() {
    assert_eq!(format!("{}", StageStatus::Skipped), "Skipped");
}
