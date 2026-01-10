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
fn test_session_status_equality() {
    assert_eq!(SessionStatus::Spawning, SessionStatus::Spawning);
    assert_eq!(SessionStatus::Running, SessionStatus::Running);
    assert_eq!(SessionStatus::Paused, SessionStatus::Paused);
    assert_eq!(SessionStatus::Completed, SessionStatus::Completed);
    assert_eq!(SessionStatus::Crashed, SessionStatus::Crashed);
    assert_eq!(
        SessionStatus::ContextExhausted,
        SessionStatus::ContextExhausted
    );

    assert_ne!(SessionStatus::Spawning, SessionStatus::Running);
    assert_ne!(SessionStatus::Running, SessionStatus::Paused);
    assert_ne!(SessionStatus::Completed, SessionStatus::Crashed);
}
