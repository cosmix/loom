//! Tests for session context tracking

use loom::models::constants::CONTEXT_WARNING_THRESHOLD;
use loom::models::session::Session;
use std::thread;
use std::time::Duration;

#[test]
fn test_session_context_tracking() {
    let mut session = Session::new();

    assert_eq!(session.context_tokens, 0);
    assert_eq!(session.context_health(), 0.0);
    assert!(!session.is_context_exhausted());

    let before = session.last_active;
    thread::sleep(Duration::from_millis(10));

    session.update_context(100_000);
    assert_eq!(session.context_tokens, 100_000);
    assert_eq!(session.context_health(), 50.0);
    assert!(!session.is_context_exhausted());
    assert!(session.last_active > before);
}

#[test]
fn test_session_context_health_calculation() {
    let mut session = Session::new();

    session.update_context(0);
    assert_eq!(session.context_health(), 0.0);

    session.update_context(50_000);
    assert_eq!(session.context_health(), 25.0);

    session.update_context(100_000);
    assert_eq!(session.context_health(), 50.0);

    session.update_context(150_000);
    assert_eq!(session.context_health(), 75.0);

    session.update_context(200_000);
    assert_eq!(session.context_health(), 100.0);
}

#[test]
fn test_session_context_exhausted_threshold() {
    let mut session = Session::new();

    session.update_context(149_999);
    assert!(!session.is_context_exhausted());

    session.update_context(150_000);
    assert_eq!(150_000_f32 / 200_000_f32, CONTEXT_WARNING_THRESHOLD);
    assert!(session.is_context_exhausted());

    session.update_context(170_000);
    assert!(session.is_context_exhausted());

    session.update_context(200_000);
    assert!(session.is_context_exhausted());
}

#[test]
fn test_session_context_health_with_zero_limit() {
    let mut session = Session::new();
    session.context_limit = 0;

    session.update_context(1000);
    assert_eq!(session.context_health(), 0.0);
    assert!(!session.is_context_exhausted());
}
