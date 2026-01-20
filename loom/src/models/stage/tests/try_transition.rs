use crate::models::stage::StageStatus;

use super::create_test_stage;

#[test]
fn test_try_transition_valid_waiting_for_deps_to_queued() {
    let status = StageStatus::WaitingForDeps;
    let result = status.try_transition(StageStatus::Queued);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), StageStatus::Queued);
}

#[test]
fn test_try_transition_invalid_completed_to_waiting_for_deps() {
    let status = StageStatus::Completed;
    let result = status.try_transition(StageStatus::WaitingForDeps);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Invalid stage status transition"));
    assert!(err.contains("Completed"));
    assert!(err.contains("WaitingForDeps"));
}

#[test]
fn test_stage_try_transition_valid() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_transition(StageStatus::Queued);
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn test_stage_try_transition_invalid() {
    let mut stage = create_test_stage(StageStatus::Completed);
    let result = stage.try_transition(StageStatus::WaitingForDeps);
    assert!(result.is_err());
    assert_eq!(stage.status, StageStatus::Completed); // Status unchanged
}

#[test]
fn test_stage_try_mark_queued_from_pending() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_mark_queued();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn test_stage_try_mark_queued_from_blocked() {
    let mut stage = create_test_stage(StageStatus::Blocked);
    let result = stage.try_mark_queued();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn test_stage_try_mark_queued_from_needs_handoff() {
    let mut stage = create_test_stage(StageStatus::NeedsHandoff);
    let result = stage.try_mark_queued();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn test_stage_try_mark_queued_invalid() {
    let mut stage = create_test_stage(StageStatus::Completed);
    let result = stage.try_mark_queued();
    assert!(result.is_err());
}

#[test]
fn test_stage_try_mark_executing_valid() {
    let mut stage = create_test_stage(StageStatus::Queued);
    let result = stage.try_mark_executing();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn test_stage_try_complete_valid() {
    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_complete(Some("Done".to_string()));
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.completed_at.is_some());
    assert_eq!(stage.close_reason, Some("Done".to_string()));
}

#[test]
fn test_stage_try_complete_invalid() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);
    let result = stage.try_complete(None);
    assert!(result.is_err());
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_stage_try_mark_blocked_valid() {
    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_mark_blocked();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::Blocked);
}

#[test]
fn test_stage_try_mark_needs_handoff_valid() {
    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_mark_needs_handoff();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::NeedsHandoff);
}

#[test]
fn test_stage_try_mark_waiting_for_input_valid() {
    let mut stage = create_test_stage(StageStatus::Executing);
    let result = stage.try_mark_waiting_for_input();
    assert!(result.is_ok());
    assert_eq!(stage.status, StageStatus::WaitingForInput);
}
