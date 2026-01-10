//! Tests for session creation and initialization

use loom::models::constants::DEFAULT_CONTEXT_LIMIT;
use loom::models::session::{Session, SessionStatus};

#[test]
fn test_session_creation() {
    let session = Session::new();

    assert!(session.id.starts_with("session-"));
    assert_eq!(session.status, SessionStatus::Spawning);
    assert_eq!(session.context_tokens, 0);
    assert_eq!(session.context_limit, DEFAULT_CONTEXT_LIMIT);
    assert_eq!(session.context_limit, 200_000);
    assert!(session.stage_id.is_none());
    assert!(session.tmux_session.is_none());
    assert!(session.worktree_path.is_none());
    assert!(session.pid.is_none());
    assert_eq!(session.created_at, session.last_active);
}

#[test]
fn test_session_id_format() {
    let session = Session::new();
    let id_parts: Vec<&str> = session.id.split('-').collect();

    // Format should be: session-{uuid}-{timestamp}
    assert_eq!(id_parts[0], "session");
    assert!(id_parts.len() >= 3);

    // UUID part should be 8 characters (first segment of UUID v4)
    assert_eq!(id_parts[1].len(), 8);

    // Timestamp should be numeric
    let timestamp_part = id_parts[2];
    assert!(timestamp_part.parse::<i64>().is_ok());
}

#[test]
fn test_session_default_trait() {
    let session = Session::default();

    assert!(session.id.starts_with("session-"));
    assert_eq!(session.status, SessionStatus::Spawning);
    assert_eq!(session.context_tokens, 0);
    assert_eq!(session.context_limit, DEFAULT_CONTEXT_LIMIT);
}

#[test]
fn test_session_clone() {
    let mut session = Session::new();
    session.assign_to_stage("stage-1".to_string());
    session.set_tmux_session("loom-test".to_string());
    session.try_mark_running().expect("Spawning -> Running");

    let cloned = session.clone();

    assert_eq!(cloned.id, session.id);
    assert_eq!(cloned.stage_id, session.stage_id);
    assert_eq!(cloned.tmux_session, session.tmux_session);
    assert_eq!(cloned.status, session.status);
}
