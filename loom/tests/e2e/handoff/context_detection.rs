//! Context detection and threshold tests for handoff system
//!
//! Thresholds are set to trigger handoff BEFORE Claude Code's automatic
//! context compaction (~75-80%), ensuring we capture full context.
//! - Green: below 50%
//! - Yellow: 50-64% (warning zone)
//! - Red: 65%+ (handoff required)

use loom::handoff::detector::{check_context_threshold, ContextLevel};
use loom::models::constants::{CONTEXT_CRITICAL_THRESHOLD, CONTEXT_WARNING_THRESHOLD, DEFAULT_CONTEXT_LIMIT};
use loom::models::session::Session;

#[test]
fn test_context_threshold_detection() {
    let mut session = Session::new();
    session.context_limit = 200_000;

    // Green zone (below 50%)
    session.context_tokens = 90_000; // 45%
    assert!(!session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Green);

    // Yellow zone (50-64%) - warning but not exhausted
    session.context_tokens = 100_000; // 50%
    assert!(!session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Yellow);

    session.context_tokens = 120_000; // 60%
    assert!(!session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Yellow);

    // Red zone (65%+) - handoff required
    session.context_tokens = 130_000; // 65%
    assert!(session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Red);

    // Well into red zone (80%)
    session.context_tokens = 160_000;
    assert!(session.is_context_exhausted());
    assert_eq!(check_context_threshold(&session), ContextLevel::Red);

    // Near Claude Code compaction zone (90%)
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
    let health = session.context_usage_percent();
    assert!((health - 50.0).abs() < 0.01);

    // 25% usage
    session.context_tokens = 50_000;
    let health = session.context_usage_percent();
    assert!((health - 25.0).abs() < 0.01);

    // 75% usage
    session.context_tokens = 150_000;
    let health = session.context_usage_percent();
    assert!((health - 75.0).abs() < 0.01);

    // 100% usage
    session.context_tokens = 200_000;
    let health = session.context_usage_percent();
    assert!((health - 100.0).abs() < 0.01);

    // Zero limit edge case
    session.context_limit = 0;
    session.context_tokens = 100;
    let health = session.context_usage_percent();
    assert_eq!(health, 0.0);
}

#[test]
fn test_context_threshold_validation() {
    let mut session = Session::new();
    session.context_limit = DEFAULT_CONTEXT_LIMIT;

    // Test the constants are correct (set before Claude Code's compaction at ~75-80%)
    assert_eq!(CONTEXT_WARNING_THRESHOLD, 0.50); // Warning starts at 50%
    assert_eq!(CONTEXT_CRITICAL_THRESHOLD, 0.65); // Handoff at 65%

    // Test boundary conditions using CRITICAL threshold (handoff trigger)
    // Use exact integer math: 200,000 * 0.65 = 130,000
    let threshold_tokens: u32 = 130_000;

    // Just below threshold (129,999 / 200,000 = 0.649995)
    session.context_tokens = threshold_tokens - 1;
    assert!(!session.is_context_exhausted());

    // Exactly at threshold (130,000 / 200,000 = 0.65)
    session.context_tokens = threshold_tokens;
    assert!(session.is_context_exhausted());

    // Just above threshold (130,001 / 200,000 = 0.650005)
    session.context_tokens = threshold_tokens + 1;
    assert!(session.is_context_exhausted());
}

#[test]
fn test_should_handoff_function() {
    let mut session = Session::new();
    session.context_limit = 200_000;

    // Green zone - should not handoff (below 50%)
    session.context_tokens = 50_000; // 25%
    assert!(!session.is_context_exhausted());

    session.context_tokens = 90_000; // 45%
    assert!(!session.is_context_exhausted());

    // Yellow zone - should not handoff yet (50-64%)
    session.context_tokens = 100_000; // 50%
    assert!(!session.is_context_exhausted());

    session.context_tokens = 120_000; // 60%
    assert!(!session.is_context_exhausted());

    // Red zone - should handoff (65%+)
    session.context_tokens = 130_000; // 65%
    assert!(session.is_context_exhausted());

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
    assert_eq!(session.context_usage_percent(), 0.0);

    session.context_tokens = 50_000;
    assert_eq!(session.context_usage_percent(), 25.0);

    session.context_tokens = 100_000;
    assert_eq!(session.context_usage_percent(), 50.0);

    session.context_tokens = 150_000;
    assert_eq!(session.context_usage_percent(), 75.0);

    session.context_tokens = 200_000;
    assert_eq!(session.context_usage_percent(), 100.0);

    // Zero limit edge case
    session.context_limit = 0;
    session.context_tokens = 1000;
    assert_eq!(session.context_usage_percent(), 0.0);
}
