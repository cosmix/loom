//! Session and stage integration tests for the handoff system.
//!
//! Context is measured in absolute resident tokens against the ceiling that
//! governs the stage, so nothing here reasons in percentages.

use loom::models::session::{Session, SessionStatus};
use loom::models::stage::{Stage, StageStatus};

#[test]
fn test_session_marks_context_exhausted() {
    let mut session = Session::new();
    session.status = SessionStatus::Running;

    session.record_heartbeat(Some(100_000), None);
    assert_eq!(session.context_tokens, 100_000);
    assert_eq!(session.status, SessionStatus::Running);

    session.record_heartbeat(Some(150_000), None);
    assert_eq!(session.context_tokens, 150_000);

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

    // Simulate the session running past its ceiling
    session.record_heartbeat(Some(160_000), None);

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
