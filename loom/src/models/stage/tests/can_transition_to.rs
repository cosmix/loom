use crate::models::stage::StageStatus;

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
fn test_queued_can_transition_to_blocked() {
    // Queued stages can be blocked due to pre-execution failures
    // (e.g., merge conflicts during base branch resolution)
    let status = StageStatus::Queued;
    assert!(status.can_transition_to(&StageStatus::Blocked));
}

#[test]
fn test_queued_cannot_transition_to_other_states() {
    let status = StageStatus::Queued;
    assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
    assert!(!status.can_transition_to(&StageStatus::Completed));
    // Note: Queued CAN transition to Blocked (pre-execution failure, e.g., merge conflict)
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
        StageStatus::MergeConflict,
        StageStatus::CompletedWithFailures,
        StageStatus::MergeBlocked,
    ];

    for status in statuses {
        assert!(
            status.can_transition_to(&status.clone()),
            "Same-state transition should be valid for {status:?}"
        );
    }
}
