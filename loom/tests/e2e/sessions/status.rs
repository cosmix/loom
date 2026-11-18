//! Tests for session status transitions

use loom::models::session::{Session, SessionStatus};

#[test]
fn test_session_status_transitions() {
    let mut session = Session::new();
    assert_eq!(session.status, SessionStatus::Spawning);

    session.try_mark_running().expect("Spawning -> Running");
    assert_eq!(session.status, SessionStatus::Running);

    session.try_mark_paused().expect("Running -> Paused");
    assert_eq!(session.status, SessionStatus::Paused);

    // Paused -> Running (resume), then Running -> Completed
    session.try_mark_running().expect("Paused -> Running");
    session.try_mark_completed().expect("Running -> Completed");
    assert_eq!(session.status, SessionStatus::Completed);
}

#[test]
fn test_session_all_status_transitions() {
    // Test pause/resume and crash workflow
    let mut session = Session::new();

    session.try_mark_running().expect("Spawning -> Running");
    assert_eq!(session.status, SessionStatus::Running);

    session.try_mark_paused().expect("Running -> Paused");
    assert_eq!(session.status, SessionStatus::Paused);

    session.try_mark_running().expect("Paused -> Running");
    assert_eq!(session.status, SessionStatus::Running);

    session.try_mark_crashed().expect("Running -> Crashed");
    assert_eq!(session.status, SessionStatus::Crashed);

    // Crashed is terminal - verify no transitions allowed
    assert!(session.try_mark_running().is_err());

    // Test context exhausted workflow
    let mut session2 = Session::new();
    session2.try_mark_running().expect("Spawning -> Running");
    session2
        .try_mark_context_exhausted()
        .expect("Running -> ContextExhausted");
    assert_eq!(session2.status, SessionStatus::ContextExhausted);

    // ContextExhausted is terminal - verify no transitions allowed
    assert!(session2.try_mark_completed().is_err());
}

#[test]
fn test_session_status_serialization() {
    for (status, expected) in [
        (SessionStatus::Spawning, "\"spawning\""),
        (SessionStatus::Running, "\"running\""),
        (SessionStatus::Paused, "\"paused\""),
        (SessionStatus::Completed, "\"completed\""),
        (SessionStatus::Crashed, "\"crashed\""),
        (SessionStatus::ContextExhausted, "\"contextexhausted\""),
    ] {
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, expected);
        assert_eq!(
            serde_json::from_str::<SessionStatus>(&json).unwrap(),
            status
        );
    }
}
