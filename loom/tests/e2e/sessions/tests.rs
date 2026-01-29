//! Tests for session attributes and serialization

use loom::models::session::Session;
use std::path::PathBuf;
use tempfile::TempDir;

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
}

#[test]
fn test_session_serialization_roundtrip() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    let file_path = temp_dir.path().join("sessions").join("test-session.json");

    let mut session = Session::new();
    session.assign_to_stage("stage-1".to_string());
    session.set_worktree_path(PathBuf::from("/tmp/test-worktree"));
    session.set_pid(54321);
    session.try_mark_running().expect("Spawning -> Running");
    session.update_context(125_000);

    std::fs::create_dir_all(file_path.parent().unwrap()).expect("Should create sessions directory");

    let json = serde_json::to_string_pretty(&session).expect("Should serialize to JSON");
    std::fs::write(&file_path, json).expect("Should write file");

    let loaded_json = std::fs::read_to_string(&file_path).expect("Should read file");
    let loaded: Session = serde_json::from_str(&loaded_json).expect("Should deserialize from JSON");

    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.stage_id, session.stage_id);
    assert_eq!(loaded.worktree_path, session.worktree_path);
    assert_eq!(loaded.pid, session.pid);
    assert_eq!(loaded.status, session.status);
    assert_eq!(loaded.context_tokens, session.context_tokens);
    assert_eq!(loaded.context_limit, session.context_limit);
    assert_eq!(loaded.created_at, session.created_at);
    assert_eq!(loaded.last_active, session.last_active);
}
