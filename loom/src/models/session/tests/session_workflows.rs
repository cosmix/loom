use super::super::*;
use super::helpers::create_test_session;

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
fn legacy_session_without_exit_reason_deserializes() {
    let session: Session = serde_yaml::from_str(
        "id: session-old\nstage_id: null\nworktree_path: null\npid: null\nstatus: running\ncontext_tokens: 0\ncreated_at: 2024-01-01T00:00:00Z\nlast_active: 2024-01-01T00:00:00Z\n",
    )
    .unwrap();

    assert_eq!(session.exit_reason, None);
}
