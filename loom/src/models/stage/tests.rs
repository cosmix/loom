use crate::models::stage::{Stage, StageStatus};

fn create_test_stage(status: StageStatus) -> Stage {
    let mut stage = Stage::new(
        "Test Stage".to_string(),
        Some("Test description".to_string()),
    );
    stage.status = status;
    stage
}

// =========================================================================
// StageStatus::can_transition_to tests
// =========================================================================

#[test]
fn test_waiting_for_deps_can_transition_to_queued() {
    let status = StageStatus::WaitingForDeps;
    assert!(status.can_transition_to(&StageStatus::Queued));
}

#[test]
fn test_waiting_for_deps_cannot_transition_to_other_states() {
    let status = StageStatus::WaitingForDeps;
    assert!(!status.can_transition_to(&StageStatus::Executing));
    assert!(!status.can_transition_to(&StageStatus::Completed));
    assert!(!status.can_transition_to(&StageStatus::Blocked));
    assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
    assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
}

#[test]
fn test_queued_can_transition_to_executing() {
    let status = StageStatus::Queued;
    assert!(status.can_transition_to(&StageStatus::Executing));
}

#[test]
fn test_queued_cannot_transition_to_other_states() {
    let status = StageStatus::Queued;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Completed));
    assert!(!status.can_transition_to(&StageStatus::Blocked));
    assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
}

#[test]
fn test_executing_can_transition_to_valid_states() {
    let status = StageStatus::Executing;
    assert!(status.can_transition_to(&StageStatus::Completed));
    assert!(status.can_transition_to(&StageStatus::Blocked));
    assert!(status.can_transition_to(&StageStatus::NeedsHandoff));
    assert!(status.can_transition_to(&StageStatus::WaitingForInput));
}

#[test]
fn test_executing_cannot_transition_to_invalid_states() {
    let status = StageStatus::Executing;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Queued));
}

#[test]
fn test_waiting_for_input_can_transition_to_executing() {
    let status = StageStatus::WaitingForInput;
    assert!(status.can_transition_to(&StageStatus::Executing));
}

#[test]
fn test_waiting_for_input_cannot_transition_to_other_states() {
    let status = StageStatus::WaitingForInput;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Queued));
    assert!(!status.can_transition_to(&StageStatus::Completed));
}

#[test]
fn test_completed_is_terminal_state() {
    let status = StageStatus::Completed;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Queued));
    assert!(!status.can_transition_to(&StageStatus::Executing));
    assert!(!status.can_transition_to(&StageStatus::Blocked));
    assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
    assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
}

#[test]
fn test_blocked_can_transition_to_queued() {
    let status = StageStatus::Blocked;
    assert!(status.can_transition_to(&StageStatus::Queued));
}

#[test]
fn test_blocked_cannot_transition_to_other_states() {
    let status = StageStatus::Blocked;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Executing));
    assert!(!status.can_transition_to(&StageStatus::Completed));
}

#[test]
fn test_needs_handoff_can_transition_to_queued() {
    let status = StageStatus::NeedsHandoff;
    assert!(status.can_transition_to(&StageStatus::Queued));
}

#[test]
fn test_needs_handoff_cannot_transition_to_other_states() {
    let status = StageStatus::NeedsHandoff;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Executing));
    assert!(!status.can_transition_to(&StageStatus::Completed));
}

#[test]
fn test_same_status_transition_is_valid() {
    let statuses = vec![
        StageStatus::WaitingForDeps,
        StageStatus::Queued,
        StageStatus::Executing,
        StageStatus::Completed,
        StageStatus::Blocked,
        StageStatus::NeedsHandoff,
        StageStatus::WaitingForInput,
        StageStatus::Skipped,
    ];

    for status in statuses {
        assert!(
            status.can_transition_to(&status.clone()),
            "Same-state transition should be valid for {status:?}"
        );
    }
}

// =========================================================================
// StageStatus::try_transition tests
// =========================================================================

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

// =========================================================================
// StageStatus::valid_transitions tests
// =========================================================================

#[test]
fn test_valid_transitions_waiting_for_deps() {
    let transitions = StageStatus::WaitingForDeps.valid_transitions();
    assert_eq!(transitions, vec![StageStatus::Queued, StageStatus::Skipped]);
}

#[test]
fn test_valid_transitions_executing() {
    let transitions = StageStatus::Executing.valid_transitions();
    assert_eq!(transitions.len(), 4);
    assert!(transitions.contains(&StageStatus::Completed));
    assert!(transitions.contains(&StageStatus::Blocked));
    assert!(transitions.contains(&StageStatus::NeedsHandoff));
    assert!(transitions.contains(&StageStatus::WaitingForInput));
}

#[test]
fn test_valid_transitions_completed() {
    let transitions = StageStatus::Completed.valid_transitions();
    assert!(transitions.is_empty());
}

// =========================================================================
// Stage::try_transition tests
// =========================================================================

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

// =========================================================================
// Full workflow tests
// =========================================================================

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

// =========================================================================
// Skipped status tests
// =========================================================================

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
fn test_valid_transitions_includes_skipped() {
    let transitions = StageStatus::WaitingForDeps.valid_transitions();
    assert!(transitions.contains(&StageStatus::Skipped));

    let transitions = StageStatus::Queued.valid_transitions();
    assert!(transitions.contains(&StageStatus::Skipped));

    let transitions = StageStatus::Blocked.valid_transitions();
    assert!(transitions.contains(&StageStatus::Skipped));

    let transitions = StageStatus::Executing.valid_transitions();
    assert!(!transitions.contains(&StageStatus::Skipped));
}

#[test]
fn test_skipped_display() {
    assert_eq!(format!("{}", StageStatus::Skipped), "Skipped");
}

#[test]
fn test_stage_new_initializes_retry_fields() {
    let stage = Stage::new("Test".to_string(), Some("Description".to_string()));
    assert_eq!(stage.retry_count, 0);
    assert_eq!(stage.max_retries, None);
    assert_eq!(stage.last_failure_at, None);
    assert_eq!(stage.failure_info, None);
}
