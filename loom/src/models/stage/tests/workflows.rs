use crate::models::stage::{Stage, StageStatus};

use super::create_test_stage;

#[test]
fn test_full_happy_path_workflow() {
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);

    // WaitingForDeps -> Queued
    assert!(stage.try_mark_queued().is_ok());
    assert_eq!(stage.status, StageStatus::Queued);

    // Queued -> Executing
    assert!(stage.try_mark_executing().is_ok());
    assert_eq!(stage.status, StageStatus::Executing);

    // Executing -> Completed (terminal state)
    assert!(stage.try_complete(None).is_ok());
    assert_eq!(stage.status, StageStatus::Completed);

    // Completed is terminal, no further transitions allowed
    assert!(stage.try_mark_queued().is_err());
}

#[test]
fn test_blocked_recovery_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> Blocked
    assert!(stage.try_mark_blocked().is_ok());
    assert_eq!(stage.status, StageStatus::Blocked);

    // Blocked -> Queued (after unblocking)
    assert!(stage.try_mark_queued().is_ok());
    assert_eq!(stage.status, StageStatus::Queued);

    // Queued -> Executing (resume)
    assert!(stage.try_mark_executing().is_ok());
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn test_handoff_recovery_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> NeedsHandoff
    assert!(stage.try_mark_needs_handoff().is_ok());
    assert_eq!(stage.status, StageStatus::NeedsHandoff);

    // NeedsHandoff -> Queued (after new session picks up)
    assert!(stage.try_mark_queued().is_ok());
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn test_waiting_for_input_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> WaitingForInput
    assert!(stage.try_mark_waiting_for_input().is_ok());
    assert_eq!(stage.status, StageStatus::WaitingForInput);

    // WaitingForInput -> Executing (input provided)
    assert!(stage.try_mark_executing().is_ok());
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn test_pre_execution_blocked_workflow() {
    // Scenario: Stage is queued but base branch resolution fails
    // (e.g., merge conflict before session even starts)
    let mut stage = create_test_stage(StageStatus::WaitingForDeps);

    // WaitingForDeps -> Queued (dependencies satisfied)
    assert!(stage.try_mark_queued().is_ok());
    assert_eq!(stage.status, StageStatus::Queued);

    // Queued -> Blocked (base resolution failed with merge conflict)
    assert!(stage.try_mark_blocked().is_ok());
    assert_eq!(stage.status, StageStatus::Blocked);

    // Blocked -> Queued (after user resolves the conflict)
    assert!(stage.try_mark_queued().is_ok());
    assert_eq!(stage.status, StageStatus::Queued);

    // Queued -> Executing (retry succeeds)
    assert!(stage.try_mark_executing().is_ok());
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn test_display_implementation() {
    assert_eq!(format!("{}", StageStatus::WaitingForDeps), "WaitingForDeps");
    assert_eq!(format!("{}", StageStatus::Queued), "Queued");
    assert_eq!(format!("{}", StageStatus::Executing), "Executing");
    assert_eq!(
        format!("{}", StageStatus::WaitingForInput),
        "WaitingForInput"
    );
    assert_eq!(format!("{}", StageStatus::Blocked), "Blocked");
    assert_eq!(format!("{}", StageStatus::Completed), "Completed");
    assert_eq!(format!("{}", StageStatus::NeedsHandoff), "NeedsHandoff");
}

#[test]
fn test_stage_auto_merge_field() {
    let mut stage = Stage::new("Test".to_string(), None);
    assert_eq!(stage.auto_merge, None);

    stage.auto_merge = Some(true);
    assert_eq!(stage.auto_merge, Some(true));
}

#[test]
fn test_stage_new_initializes_retry_fields() {
    let stage = Stage::new("Test".to_string(), Some("Description".to_string()));
    assert_eq!(stage.retry_count, 0);
    assert_eq!(stage.max_retries, None);
    assert_eq!(stage.last_failure_at, None);
    assert_eq!(stage.failure_info, None);
}

#[test]
fn test_stage_new_initializes_fix_attempt_fields() {
    let stage = Stage::new("Test".to_string(), Some("Description".to_string()));
    assert_eq!(stage.fix_attempts, 0);
    assert_eq!(stage.max_fix_attempts, None);
    assert_eq!(stage.review_reason, None);
}

#[test]
fn test_human_review_approve_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> NeedsHumanReview
    assert!(stage
        .try_request_human_review("Suspicious changes detected".to_string())
        .is_ok());
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(
        stage.review_reason,
        Some("Suspicious changes detected".to_string())
    );

    // NeedsHumanReview -> Executing (approved)
    assert!(stage.try_approve_review().is_ok());
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.review_reason, None);
}

#[test]
fn test_human_review_reject_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);

    // Executing -> NeedsHumanReview
    assert!(stage
        .try_request_human_review("Code quality issues".to_string())
        .is_ok());
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);

    // NeedsHumanReview -> Blocked (rejected)
    assert!(stage
        .try_reject_review("Needs refactoring".to_string())
        .is_ok());
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.review_reason, Some("Needs refactoring".to_string()));
}

#[test]
fn test_human_review_force_complete_workflow() {
    let mut stage = create_test_stage(StageStatus::Executing);
    stage.started_at = Some(chrono::Utc::now());

    // Executing -> NeedsHumanReview
    assert!(stage
        .try_request_human_review("Minor issues found".to_string())
        .is_ok());
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);

    // NeedsHumanReview -> Completed (force-completed)
    assert!(stage.try_force_complete_review().is_ok());
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.completed_at.is_some());
    assert!(stage.duration_secs.is_some());
}

#[test]
fn test_fix_attempts_tracking() {
    let mut stage = Stage::new("Test".to_string(), None);

    assert_eq!(stage.fix_attempts, 0);
    assert!(!stage.is_at_fix_limit());
    assert_eq!(stage.get_effective_max_fix_attempts(), 3);

    assert_eq!(stage.increment_fix_attempts(), 1);
    assert_eq!(stage.increment_fix_attempts(), 2);
    assert!(!stage.is_at_fix_limit());

    assert_eq!(stage.increment_fix_attempts(), 3);
    assert!(stage.is_at_fix_limit());
}

#[test]
fn test_fix_attempts_custom_limit() {
    let mut stage = Stage::new("Test".to_string(), None);
    stage.max_fix_attempts = Some(2);

    assert_eq!(stage.get_effective_max_fix_attempts(), 2);
    assert!(!stage.is_at_fix_limit());

    stage.increment_fix_attempts();
    assert!(!stage.is_at_fix_limit());

    stage.increment_fix_attempts();
    assert!(stage.is_at_fix_limit());
}

#[test]
fn test_display_needs_human_review() {
    assert_eq!(
        format!("{}", StageStatus::NeedsHumanReview),
        "NeedsHumanReview"
    );
}

#[test]
fn test_needs_human_review_request_from_invalid_state() {
    let mut stage = create_test_stage(StageStatus::Queued);
    assert!(stage.try_request_human_review("test".to_string()).is_err());
}
