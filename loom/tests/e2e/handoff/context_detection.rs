//! Context detection and threshold tests for handoff system

use loom::handoff::detector::{check_context_threshold, ContextLevel};
use loom::models::constants::{CONTEXT_WARNING_THRESHOLD, DEFAULT_CONTEXT_LIMIT};
use loom::models::session::Session;

#[test]
fn test_context_threshold_detection() {
    let mut session = Session::new();
    session.context_limit = 200_000;

    // Below threshold (50%)
    session.context_tokens = 100_000;
    assert!(!session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Green);

    // Just below threshold (74%)
    session.context_tokens = 148_000;
    assert!(!session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Yellow);

    // At threshold (75%)
    session.context_tokens = 150_000;
    assert!(session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Red);

    // Above threshold (80%)
    session.context_tokens = 160_000;
    assert!(session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Red);

    // Well above threshold (90%)
    session.context_tokens = 180_000;
    assert!(session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Red);
}

#[test]
fn test_context_health_calculation() {
    let mut session = Session::new();
    session.context_limit = 200_000;

    // 50% usage
    session.context_tokens = 100_000;
    let health = session.context_health();
    assert!((health - 50.0).abs() < 0.01);

    // 25% usage
    session.context_tokens = 50_000;
    let health = session.context_health();
    assert!((health - 25.0).abs() < 0.01);

    // 75% usage
    session.context_tokens = 150_000;
    let health = session.context_health();
    assert!((health - 75.0).abs() < 0.01);

    // 100% usage
    session.context_tokens = 200_000;
    let health = session.context_health();
    assert!((health - 100.0).abs() < 0.01);

    // Zero limit edge case
    session.context_limit = 0;
    session.context_tokens = 100;
    let health = session.context_health();
    assert_eq!(health, 0.0);
}

#[test]
fn test_context_threshold_validation() {
    let mut session = Session::new();
    session.context_limit = DEFAULT_CONTEXT_LIMIT;

    // Test the constant is correct
    assert_eq!(CONTEXT_WARNING_THRESHOLD, 0.75);

    // Test boundary conditions
    let threshold_tokens = (DEFAULT_CONTEXT_LIMIT as f32 * CONTEXT_WARNING_THRESHOLD) as u32;

    // Just below threshold
    session.context_tokens = threshold_tokens - 1;
    assert!(!session.is_context_exhausted());

    // Exactly at threshold
    session.context_tokens = threshold_tokens;
    assert!(session.is_context_exhausted());

    // Just above threshold
    session.context_tokens = threshold_tokens + 1;
    assert!(session.is_context_exhausted());
}

#[test]
fn test_should_handoff_function() {
    let mut session = Session::new();
    session.context_limit = 200_000;

    // Green zone - should not handoff
    session.context_tokens = 50_000; // 25%
    assert!(!session.is_context_exhausted());

    session.context_tokens = 100_000; // 50%
    assert!(!session.is_context_exhausted());

    // Yellow zone - should not handoff yet
    session.context_tokens = 130_000; // 65%
    assert!(!session.is_context_exhausted());

    // Red zone - should handoff
    session.context_tokens = 150_000; // 75%
    assert!(session.is_context_exhausted());

    session.context_tokens = 180_000; // 90%
    assert!(session.is_context_exhausted());
}

#[test]
fn test_context_usage_percent_function() {
    let mut session = Session::new();
    session.context_limit = 200_000;

    session.context_tokens = 0;
    assert_eq!(session.context_health(), 0.0);

    session.context_tokens = 50_000;
    assert_eq!(session.context_health(), 25.0);

    session.context_tokens = 100_000;
    assert_eq!(session.context_health(), 50.0);

    session.context_tokens = 150_000;
    assert_eq!(session.context_health(), 75.0);

    session.context_tokens = 200_000;
    assert_eq!(session.context_health(), 100.0);

    // Zero limit edge case
    session.context_limit = 0;
    session.context_tokens = 1000;
    assert_eq!(session.context_health(), 0.0);
}
