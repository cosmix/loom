use super::super::*;

// =========================================================================
// SessionStatus::can_transition_to tests
// =========================================================================

#[test]
fn test_spawning_can_transition_to_running() {
    let status = SessionStatus::Spawning;
    assert!(status.can_transition_to(&SessionStatus::Running));
}

#[test]
fn test_spawning_cannot_transition_to_other_states() {
    let status = SessionStatus::Spawning;
    assert!(!status.can_transition_to(&SessionStatus::Paused));
    assert!(!status.can_transition_to(&SessionStatus::Completed));
    assert!(!status.can_transition_to(&SessionStatus::Crashed));
    assert!(!status.can_transition_to(&SessionStatus::ContextExhausted));
}

#[test]
fn test_running_can_transition_to_valid_states() {
    let status = SessionStatus::Running;
    assert!(status.can_transition_to(&SessionStatus::Completed));
    assert!(status.can_transition_to(&SessionStatus::Paused));
    assert!(status.can_transition_to(&SessionStatus::Crashed));
    assert!(status.can_transition_to(&SessionStatus::ContextExhausted));
}

#[test]
fn test_running_cannot_transition_to_spawning() {
    let status = SessionStatus::Running;
    assert!(!status.can_transition_to(&SessionStatus::Spawning));
}

#[test]
fn test_paused_can_transition_to_running() {
    let status = SessionStatus::Paused;
    assert!(status.can_transition_to(&SessionStatus::Running));
}

#[test]
fn test_paused_cannot_transition_to_other_states() {
    let status = SessionStatus::Paused;
    assert!(!status.can_transition_to(&SessionStatus::Spawning));
    assert!(!status.can_transition_to(&SessionStatus::Completed));
    assert!(!status.can_transition_to(&SessionStatus::Crashed));
    assert!(!status.can_transition_to(&SessionStatus::ContextExhausted));
}

#[test]
fn test_completed_is_terminal_state() {
    let status = SessionStatus::Completed;
    assert!(!status.can_transition_to(&SessionStatus::Spawning));
    assert!(!status.can_transition_to(&SessionStatus::Running));
    assert!(!status.can_transition_to(&SessionStatus::Paused));
    assert!(!status.can_transition_to(&SessionStatus::Crashed));
    assert!(!status.can_transition_to(&SessionStatus::ContextExhausted));
}

#[test]
fn test_crashed_is_terminal_state() {
    let status = SessionStatus::Crashed;
    assert!(!status.can_transition_to(&SessionStatus::Spawning));
    assert!(!status.can_transition_to(&SessionStatus::Running));
    assert!(!status.can_transition_to(&SessionStatus::Paused));
    assert!(!status.can_transition_to(&SessionStatus::Completed));
    assert!(!status.can_transition_to(&SessionStatus::ContextExhausted));
}

#[test]
fn test_context_exhausted_is_terminal_state() {
    let status = SessionStatus::ContextExhausted;
    assert!(!status.can_transition_to(&SessionStatus::Spawning));
    assert!(!status.can_transition_to(&SessionStatus::Running));
    assert!(!status.can_transition_to(&SessionStatus::Paused));
    assert!(!status.can_transition_to(&SessionStatus::Completed));
    assert!(!status.can_transition_to(&SessionStatus::Crashed));
}

#[test]
fn test_same_status_transition_is_valid() {
    let statuses = vec![
        SessionStatus::Spawning,
        SessionStatus::Running,
        SessionStatus::Paused,
        SessionStatus::Completed,
        SessionStatus::Crashed,
        SessionStatus::ContextExhausted,
    ];

    for status in statuses {
        assert!(
            status.can_transition_to(&status.clone()),
            "Same-state transition should be valid for {status:?}"
        );
    }
}

// =========================================================================
// SessionStatus::try_transition tests
// =========================================================================

#[test]
fn test_try_transition_valid_spawning_to_running() {
    let status = SessionStatus::Spawning;
    let result = status.try_transition(SessionStatus::Running);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SessionStatus::Running);
}

#[test]
fn test_try_transition_invalid_completed_to_running() {
    let status = SessionStatus::Completed;
    let result = status.try_transition(SessionStatus::Running);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Invalid session status transition"));
    assert!(err.contains("Completed"));
    assert!(err.contains("Running"));
}

// =========================================================================
// SessionStatus::valid_transitions tests
// =========================================================================

#[test]
fn test_valid_transitions_spawning() {
    let transitions = SessionStatus::Spawning.valid_transitions();
    assert_eq!(transitions, vec![SessionStatus::Running]);
}

#[test]
fn test_valid_transitions_running() {
    let transitions = SessionStatus::Running.valid_transitions();
    assert_eq!(transitions.len(), 4);
    assert!(transitions.contains(&SessionStatus::Completed));
    assert!(transitions.contains(&SessionStatus::Paused));
    assert!(transitions.contains(&SessionStatus::Crashed));
    assert!(transitions.contains(&SessionStatus::ContextExhausted));
}

#[test]
fn test_valid_transitions_terminal_states() {
    assert!(SessionStatus::Completed.valid_transitions().is_empty());
    assert!(SessionStatus::Crashed.valid_transitions().is_empty());
    assert!(SessionStatus::ContextExhausted
        .valid_transitions()
        .is_empty());
}

// =========================================================================
// SessionStatus::is_terminal tests
// =========================================================================

#[test]
fn test_is_terminal_true_for_terminal_states() {
    assert!(SessionStatus::Completed.is_terminal());
    assert!(SessionStatus::Crashed.is_terminal());
    assert!(SessionStatus::ContextExhausted.is_terminal());
}

#[test]
fn test_is_terminal_false_for_non_terminal_states() {
    assert!(!SessionStatus::Spawning.is_terminal());
    assert!(!SessionStatus::Running.is_terminal());
    assert!(!SessionStatus::Paused.is_terminal());
}

// =========================================================================
// Display implementation test
// =========================================================================

#[test]
fn test_display_implementation() {
    assert_eq!(format!("{}", SessionStatus::Spawning), "Spawning");
    assert_eq!(format!("{}", SessionStatus::Running), "Running");
    assert_eq!(format!("{}", SessionStatus::Paused), "Paused");
    assert_eq!(format!("{}", SessionStatus::Completed), "Completed");
    assert_eq!(format!("{}", SessionStatus::Crashed), "Crashed");
    assert_eq!(
        format!("{}", SessionStatus::ContextExhausted),
        "ContextExhausted"
    );
}
