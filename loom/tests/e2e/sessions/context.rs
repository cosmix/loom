//! Tests for session context tracking
//!
//! Thresholds are set to trigger handoff BEFORE Claude Code's automatic
//! context compaction (~75-80%), ensuring we capture full context.
//! - Green: below 50%
//! - Yellow: 50-64% (warning zone, not exhausted)
//! - Red: 65%+ (handoff required, exhausted)

use loom::models::constants::CONTEXT_CRITICAL_THRESHOLD;
use loom::models::session::Session;
use std::thread;
use std::time::Duration;

#[test]
fn test_session_context_tracking() {
    let mut session = Session::new();

    assert_eq!(session.context_tokens, 0);
    assert_eq!(session.context_usage_percent(), 0.0);
    assert!(!session.is_context_exhausted());

    let before = session.last_active;
    thread::sleep(Duration::from_millis(10));

    // 50% is in the yellow zone (warning) but not exhausted
    session.update_context(100_000);
    assert_eq!(session.context_tokens, 100_000);
    assert_eq!(session.context_usage_percent(), 50.0);
    assert!(!session.is_context_exhausted()); // Yellow zone, not exhausted yet
    assert!(session.last_active > before);
}

#[test]
fn test_session_context_health_calculation() {
    let mut session = Session::new();

    session.update_context(0);
    assert_eq!(session.context_usage_percent(), 0.0);

    session.update_context(50_000);
    assert_eq!(session.context_usage_percent(), 25.0);

    session.update_context(100_000);
    assert_eq!(session.context_usage_percent(), 50.0);

    session.update_context(150_000);
    assert_eq!(session.context_usage_percent(), 75.0);

    session.update_context(200_000);
    assert_eq!(session.context_usage_percent(), 100.0);
}

#[test]
fn test_session_context_exhausted_threshold() {
    let mut session = Session::new();

    // Just below 65% critical threshold
    session.update_context(129_999);
    assert!(!session.is_context_exhausted());

    // At 65% critical threshold - handoff required
    session.update_context(130_000);
    assert_eq!(130_000_f32 / 200_000_f32, CONTEXT_CRITICAL_THRESHOLD);
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
    assert_eq!(session.context_usage_percent(), 0.0);
    assert!(!session.is_context_exhausted());
}
