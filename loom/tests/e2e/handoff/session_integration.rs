//! Session and stage integration tests for handoff system
//!
//! Thresholds are set to trigger handoff BEFORE Claude Code's automatic
//! context compaction (~75-80%), ensuring we capture full context.

use loom::models::constants::DEFAULT_CONTEXT_LIMIT;
use loom::models::session::{Session, SessionStatus};
use loom::models::stage::{Stage, StageStatus};

#[test]
fn test_session_marks_context_exhausted() {
    let mut session = Session::new();
    session.context_limit = DEFAULT_CONTEXT_LIMIT;
    session.status = SessionStatus::Running;

    // Simulate gradual context usage increase
    // Yellow zone (50-64%) - not exhausted yet
    session.context_tokens = 100_000; // 50%
    assert!(!session.is_context_exhausted());
    assert_eq!(session.status, SessionStatus::Running);

    // Increase to critical threshold (65%)
    session.context_tokens = 130_000; // 65%
    assert!(session.is_context_exhausted());

    // Manually mark session as context exhausted
    session
        .try_mark_context_exhausted()
        .expect("Should transition to ContextExhausted");
    assert_eq!(session.status, SessionStatus::ContextExhausted);
}

#[test]
fn test_context_exhausted_triggers_stage_needs_handoff() {
    let mut session = Session::new();
    let mut stage = Stage::new("test-stage".to_string(), None);

    // Initial states
    session.status = SessionStatus::Running;
    stage.status = StageStatus::Executing;

    // Simulate context exhaustion
    session.context_limit = DEFAULT_CONTEXT_LIMIT;
    session.context_tokens = 160_000; // 80%

    assert!(session.is_context_exhausted());

    // Update statuses using validated transitions
    session
        .try_mark_context_exhausted()
        .expect("Should transition to ContextExhausted");
    stage
        .try_mark_needs_handoff()
        .expect("Should transition to NeedsHandoff");

    assert_eq!(session.status, SessionStatus::ContextExhausted);
    assert_eq!(stage.status, StageStatus::NeedsHandoff);
}
