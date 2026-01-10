use super::*;

fn create_test_session(status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.status = status;
    session
}

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
// Session::try_transition tests
// =========================================================================

#[test]
fn test_session_try_transition_valid() {
    let mut session = create_test_session(SessionStatus::Spawning);
    let result = session.try_transition(SessionStatus::Running);
    assert!(result.is_ok());
    assert_eq!(session.status, SessionStatus::Running);
}

#[test]
fn test_session_try_transition_invalid() {
    let mut session = create_test_session(SessionStatus::Completed);
    let result = session.try_transition(SessionStatus::Running);
    assert!(result.is_err());
    assert_eq!(session.status, SessionStatus::Completed); // Status unchanged
}

#[test]
fn test_session_try_mark_running_from_spawning() {
    let mut session = create_test_session(SessionStatus::Spawning);
    let result = session.try_mark_running();
    assert!(result.is_ok());
    assert_eq!(session.status, SessionStatus::Running);
}

#[test]
fn test_session_try_mark_running_from_paused() {
    let mut session = create_test_session(SessionStatus::Paused);
    let result = session.try_mark_running();
    assert!(result.is_ok());
    assert_eq!(session.status, SessionStatus::Running);
}

#[test]
fn test_session_try_mark_running_invalid() {
    let mut session = create_test_session(SessionStatus::Completed);
    let result = session.try_mark_running();
    assert!(result.is_err());
}

#[test]
fn test_session_try_mark_paused_valid() {
    let mut session = create_test_session(SessionStatus::Running);
    let result = session.try_mark_paused();
    assert!(result.is_ok());
    assert_eq!(session.status, SessionStatus::Paused);
}

#[test]
fn test_session_try_mark_paused_invalid() {
    let mut session = create_test_session(SessionStatus::Spawning);
    let result = session.try_mark_paused();
    assert!(result.is_err());
}

#[test]
fn test_session_try_mark_completed_valid() {
    let mut session = create_test_session(SessionStatus::Running);
    let result = session.try_mark_completed();
    assert!(result.is_ok());
    assert_eq!(session.status, SessionStatus::Completed);
}

#[test]
fn test_session_try_mark_crashed_valid() {
    let mut session = create_test_session(SessionStatus::Running);
    let result = session.try_mark_crashed();
    assert!(result.is_ok());
    assert_eq!(session.status, SessionStatus::Crashed);
}

#[test]
fn test_session_try_mark_context_exhausted_valid() {
    let mut session = create_test_session(SessionStatus::Running);
    let result = session.try_mark_context_exhausted();
    assert!(result.is_ok());
    assert_eq!(session.status, SessionStatus::ContextExhausted);
}

// =========================================================================
// Full workflow tests
// =========================================================================

#[test]
fn test_full_happy_path_workflow() {
    let mut session = create_test_session(SessionStatus::Spawning);

    // Spawning -> Running
    assert!(session.try_mark_running().is_ok());
    assert_eq!(session.status, SessionStatus::Running);

    // Running -> Completed
    assert!(session.try_mark_completed().is_ok());
    assert_eq!(session.status, SessionStatus::Completed);
}

#[test]
fn test_pause_resume_workflow() {
    let mut session = create_test_session(SessionStatus::Running);

    // Running -> Paused
    assert!(session.try_mark_paused().is_ok());
    assert_eq!(session.status, SessionStatus::Paused);

    // Paused -> Running
    assert!(session.try_mark_running().is_ok());
    assert_eq!(session.status, SessionStatus::Running);
}

#[test]
fn test_crash_workflow() {
    let mut session = create_test_session(SessionStatus::Running);

    // Running -> Crashed
    assert!(session.try_mark_crashed().is_ok());
    assert_eq!(session.status, SessionStatus::Crashed);

    // Crashed is terminal - cannot recover
    assert!(session.try_mark_running().is_err());
}

#[test]
fn test_context_exhausted_workflow() {
    let mut session = create_test_session(SessionStatus::Running);

    // Running -> ContextExhausted
    assert!(session.try_mark_context_exhausted().is_ok());
    assert_eq!(session.status, SessionStatus::ContextExhausted);

    // ContextExhausted is terminal - cannot recover
    assert!(session.try_mark_running().is_err());
}

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

// =========================================================================
// SessionType tests
// =========================================================================

#[test]
fn test_session_type_default() {
    let session_type = SessionType::default();
    assert_eq!(session_type, SessionType::Stage);
}

#[test]
fn test_session_type_display() {
    assert_eq!(format!("{}", SessionType::Stage), "stage");
    assert_eq!(format!("{}", SessionType::Merge), "merge");
}

// =========================================================================
// Merge session tests
// =========================================================================

#[test]
fn test_new_merge_session() {
    let session = Session::new_merge("loom/feature".to_string(), "main".to_string());

    assert_eq!(session.session_type, SessionType::Merge);
    assert!(session.is_merge_session());
    assert_eq!(
        session.merge_source_branch,
        Some("loom/feature".to_string())
    );
    assert_eq!(session.merge_target_branch, Some("main".to_string()));
    assert_eq!(session.status, SessionStatus::Spawning);
}

#[test]
fn test_regular_session_is_not_merge() {
    let session = Session::new();

    assert_eq!(session.session_type, SessionType::Stage);
    assert!(!session.is_merge_session());
    assert!(session.merge_source_branch.is_none());
    assert!(session.merge_target_branch.is_none());
}

#[test]
fn test_merge_session_serialization() {
    let session = Session::new_merge("loom/stage-1".to_string(), "develop".to_string());

    // Test that serialization works
    let json = serde_json::to_string(&session).expect("Failed to serialize");
    assert!(json.contains("\"session_type\":\"merge\""));
    assert!(json.contains("\"merge_source_branch\":\"loom/stage-1\""));
    assert!(json.contains("\"merge_target_branch\":\"develop\""));

    // Test that deserialization works
    let deserialized: Session = serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.session_type, SessionType::Merge);
    assert!(deserialized.is_merge_session());
    assert_eq!(
        deserialized.merge_source_branch,
        Some("loom/stage-1".to_string())
    );
    assert_eq!(
        deserialized.merge_target_branch,
        Some("develop".to_string())
    );
}

#[test]
fn test_regular_session_serialization_omits_merge_fields() {
    let session = Session::new();

    let json = serde_json::to_string(&session).expect("Failed to serialize");

    // merge_source_branch and merge_target_branch should be omitted (skip_serializing_if)
    assert!(!json.contains("merge_source_branch"));
    assert!(!json.contains("merge_target_branch"));
    // But session_type should still be present (defaults to stage)
    assert!(json.contains("\"session_type\":\"stage\""));
}

#[test]
fn test_deserialize_legacy_session_without_merge_fields() {
    // Simulate deserializing a session that was created before merge fields existed
    let legacy_json = r#"{
        "id": "session-abc123-1234567890",
        "stage_id": null,
        "tmux_session": null,
        "worktree_path": null,
        "pid": null,
        "status": "spawning",
        "context_tokens": 0,
        "context_limit": 200000,
        "created_at": "2024-01-01T00:00:00Z",
        "last_active": "2024-01-01T00:00:00Z"
    }"#;

    let session: Session =
        serde_json::from_str(legacy_json).expect("Failed to deserialize legacy session");

    // Default values should be applied
    assert_eq!(session.session_type, SessionType::Stage);
    assert!(!session.is_merge_session());
    assert!(session.merge_source_branch.is_none());
    assert!(session.merge_target_branch.is_none());
}
