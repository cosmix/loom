//! Tests for session context tracking.
//!
//! Context is an absolute count of resident tokens, taken from the heartbeat
//! the hooks write. The session record holds the measurement; the ceiling that
//! judges it belongs to the stage, not to the session.

use loom::models::session::Session;
use std::thread;
use std::time::Duration;

#[test]
fn test_session_context_tracking() {
    let mut session = Session::new();
    assert_eq!(session.context_tokens, 0);
    assert_eq!(session.transcript_path, None);

    let before = session.last_active;
    thread::sleep(Duration::from_millis(10));

    session.record_heartbeat(Some(100_000), Some("/t/session.jsonl".to_string()));

    assert_eq!(session.context_tokens, 100_000);
    assert_eq!(
        session.transcript_path,
        Some("/t/session.jsonl".to_string())
    );
    assert!(session.last_active > before);
}

/// Every heartbeat advances `last_active`, whether or not it carried a reading.
/// Before this held, a session that had been working for hours still reported
/// its spawn timestamp and read as idle.
#[test]
fn test_every_heartbeat_advances_last_active() {
    let mut session = Session::new();
    let before = session.last_active;
    thread::sleep(Duration::from_millis(10));

    session.record_heartbeat(None, None);

    assert!(session.last_active > before);
}

/// A `None` reading is the hook saying it could not measure the transcript.
/// Treating that as zero would silently retract a handoff already due.
#[test]
fn test_unmeasured_heartbeat_preserves_the_last_reading() {
    let mut session = Session::new();

    session.record_heartbeat(Some(147_000), None);
    session.record_heartbeat(None, None);
    session.record_heartbeat(None, None);

    assert_eq!(session.context_tokens, 147_000);
}

/// The transcript path is recorded the first time it arrives and never nulled.
#[test]
fn test_transcript_path_is_never_cleared() {
    let mut session = Session::new();

    session.record_heartbeat(None, Some("/t/a.jsonl".to_string()));
    session.record_heartbeat(Some(50_000), None);

    assert_eq!(session.transcript_path, Some("/t/a.jsonl".to_string()));
}
