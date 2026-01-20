use super::super::*;
use super::helpers::create_test_session;

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
