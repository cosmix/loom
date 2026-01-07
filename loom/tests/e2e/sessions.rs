//! End-to-end tests for session lifecycle and management

use loom::models::constants::{CONTEXT_WARNING_THRESHOLD, DEFAULT_CONTEXT_LIMIT};
use loom::models::session::{Session, SessionStatus};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

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
fn test_session_stage_assignment() {
    let mut session = Session::new();
    let before = session.last_active;

    thread::sleep(Duration::from_millis(10));
    session.assign_to_stage("stage-1".to_string());

    assert_eq!(session.stage_id, Some("stage-1".to_string()));
    assert!(session.last_active > before);
}

#[test]
fn test_session_release_from_stage() {
    let mut session = Session::new();
    session.assign_to_stage("stage-1".to_string());
    assert_eq!(session.stage_id, Some("stage-1".to_string()));

    let before = session.last_active;
    thread::sleep(Duration::from_millis(10));
    session.release_from_stage();

    assert!(session.stage_id.is_none());
    assert!(session.last_active > before);
}

#[test]
fn test_session_status_transitions() {
    let mut session = Session::new();
    assert_eq!(session.status, SessionStatus::Spawning);

    session.mark_running();
    assert_eq!(session.status, SessionStatus::Running);

    session.mark_paused();
    assert_eq!(session.status, SessionStatus::Paused);

    session.mark_completed();
    assert_eq!(session.status, SessionStatus::Completed);
}

#[test]
fn test_session_all_status_transitions() {
    let mut session = Session::new();

    session.mark_running();
    assert_eq!(session.status, SessionStatus::Running);

    session.mark_paused();
    assert_eq!(session.status, SessionStatus::Paused);

    session.mark_running();
    assert_eq!(session.status, SessionStatus::Running);

    session.mark_crashed();
    assert_eq!(session.status, SessionStatus::Crashed);

    let mut session2 = Session::new();
    session2.mark_context_exhausted();
    assert_eq!(session2.status, SessionStatus::ContextExhausted);

    session2.mark_completed();
    assert_eq!(session2.status, SessionStatus::Completed);
}

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
    assert_eq!(
        150_000_f32 / 200_000_f32,
        CONTEXT_WARNING_THRESHOLD
    );
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

#[test]
fn test_session_tmux_assignment() {
    let mut session = Session::new();
    assert!(session.tmux_session.is_none());

    session.set_tmux_session("loom-session-123".to_string());
    assert_eq!(
        session.tmux_session,
        Some("loom-session-123".to_string())
    );
}

#[test]
fn test_session_worktree_path_assignment() {
    let mut session = Session::new();
    assert!(session.worktree_path.is_none());

    let path = PathBuf::from("/tmp/loom-worktree-123");
    session.set_worktree_path(path.clone());
    assert_eq!(session.worktree_path, Some(path));
}

#[test]
fn test_session_pid_assignment() {
    let mut session = Session::new();
    assert!(session.pid.is_none());

    session.set_pid(12345);
    assert_eq!(session.pid, Some(12345));

    session.clear_pid();
    assert!(session.pid.is_none());
}

#[test]
fn test_session_serialization_roundtrip() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    let file_path = temp_dir.path().join("sessions").join("test-session.json");

    let mut session = Session::new();
    session.assign_to_stage("stage-1".to_string());
    session.set_tmux_session("loom-test-123".to_string());
    session.set_worktree_path(PathBuf::from("/tmp/test-worktree"));
    session.set_pid(54321);
    session.mark_running();
    session.update_context(125_000);

    std::fs::create_dir_all(file_path.parent().unwrap())
        .expect("Should create sessions directory");

    let json = serde_json::to_string_pretty(&session).expect("Should serialize to JSON");
    std::fs::write(&file_path, json).expect("Should write file");

    let loaded_json = std::fs::read_to_string(&file_path).expect("Should read file");
    let loaded: Session = serde_json::from_str(&loaded_json).expect("Should deserialize from JSON");

    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.stage_id, session.stage_id);
    assert_eq!(loaded.tmux_session, session.tmux_session);
    assert_eq!(loaded.worktree_path, session.worktree_path);
    assert_eq!(loaded.pid, session.pid);
    assert_eq!(loaded.status, session.status);
    assert_eq!(loaded.context_tokens, session.context_tokens);
    assert_eq!(loaded.context_limit, session.context_limit);
    assert_eq!(loaded.created_at, session.created_at);
    assert_eq!(loaded.last_active, session.last_active);
}

#[test]
fn test_session_clone() {
    let mut session = Session::new();
    session.assign_to_stage("stage-1".to_string());
    session.set_tmux_session("loom-test".to_string());
    session.mark_running();

    let cloned = session.clone();

    assert_eq!(cloned.id, session.id);
    assert_eq!(cloned.stage_id, session.stage_id);
    assert_eq!(cloned.tmux_session, session.tmux_session);
    assert_eq!(cloned.status, session.status);
}

#[test]
fn test_multiple_sessions_independent() {
    let mut session1 = Session::new();
    let mut session2 = Session::new();
    let mut session3 = Session::new();

    session1.assign_to_stage("stage-1".to_string());
    session2.assign_to_stage("stage-2".to_string());
    session3.assign_to_stage("stage-3".to_string());

    session1.mark_running();
    session2.mark_paused();
    session3.mark_completed();

    session1.update_context(50_000);
    session2.update_context(100_000);
    session3.update_context(150_000);

    assert_ne!(session1.id, session2.id);
    assert_ne!(session2.id, session3.id);
    assert_ne!(session1.id, session3.id);

    assert_eq!(session1.stage_id, Some("stage-1".to_string()));
    assert_eq!(session2.stage_id, Some("stage-2".to_string()));
    assert_eq!(session3.stage_id, Some("stage-3".to_string()));

    assert_eq!(session1.status, SessionStatus::Running);
    assert_eq!(session2.status, SessionStatus::Paused);
    assert_eq!(session3.status, SessionStatus::Completed);

    assert_eq!(session1.context_tokens, 50_000);
    assert_eq!(session2.context_tokens, 100_000);
    assert_eq!(session3.context_tokens, 150_000);
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
fn test_session_stage_reassignment() {
    let mut session = Session::new();

    session.assign_to_stage("stage-1".to_string());
    assert_eq!(session.stage_id, Some("stage-1".to_string()));
    let time1 = session.last_active;

    thread::sleep(Duration::from_millis(10));
    session.assign_to_stage("stage-2".to_string());
    assert_eq!(session.stage_id, Some("stage-2".to_string()));
    assert!(session.last_active > time1);
}

#[test]
fn test_session_complex_lifecycle() {
    let mut session = Session::new();

    assert_eq!(session.status, SessionStatus::Spawning);

    session.assign_to_stage("stage-1".to_string());
    session.set_tmux_session("loom-stage-1".to_string());
    session.set_worktree_path(PathBuf::from("/tmp/worktree-1"));
    session.set_pid(99999);

    session.mark_running();
    assert_eq!(session.status, SessionStatus::Running);

    session.update_context(50_000);
    assert_eq!(session.context_health(), 25.0);
    assert!(!session.is_context_exhausted());

    session.update_context(150_000);
    assert_eq!(session.context_health(), 75.0);
    assert!(session.is_context_exhausted());

    session.mark_context_exhausted();
    assert_eq!(session.status, SessionStatus::ContextExhausted);

    session.release_from_stage();
    assert!(session.stage_id.is_none());

    session.clear_pid();
    assert!(session.pid.is_none());
}

#[test]
fn test_session_timestamps_update_correctly() {
    let mut session = Session::new();
    let created = session.created_at;
    let initial_active = session.last_active;

    assert_eq!(created, initial_active);

    thread::sleep(Duration::from_millis(10));
    session.assign_to_stage("stage-1".to_string());
    assert_eq!(session.created_at, created);
    assert!(session.last_active > initial_active);

    let after_assign = session.last_active;
    thread::sleep(Duration::from_millis(10));
    session.update_context(1000);
    assert_eq!(session.created_at, created);
    assert!(session.last_active > after_assign);

    let after_update = session.last_active;
    thread::sleep(Duration::from_millis(10));
    session.release_from_stage();
    assert_eq!(session.created_at, created);
    assert!(session.last_active > after_update);
}

#[test]
fn test_session_status_equality() {
    assert_eq!(SessionStatus::Spawning, SessionStatus::Spawning);
    assert_eq!(SessionStatus::Running, SessionStatus::Running);
    assert_eq!(SessionStatus::Paused, SessionStatus::Paused);
    assert_eq!(SessionStatus::Completed, SessionStatus::Completed);
    assert_eq!(SessionStatus::Crashed, SessionStatus::Crashed);
    assert_eq!(SessionStatus::ContextExhausted, SessionStatus::ContextExhausted);

    assert_ne!(SessionStatus::Spawning, SessionStatus::Running);
    assert_ne!(SessionStatus::Running, SessionStatus::Paused);
    assert_ne!(SessionStatus::Completed, SessionStatus::Crashed);
}
