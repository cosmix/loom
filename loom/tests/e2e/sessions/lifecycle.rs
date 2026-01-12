//! Tests for session lifecycle management

use loom::models::session::{Session, SessionStatus};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

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
    session.set_worktree_path(PathBuf::from("/tmp/worktree-1"));
    session.set_pid(99999);

    session.try_mark_running().expect("Spawning -> Running");
    assert_eq!(session.status, SessionStatus::Running);

    session.update_context(50_000);
    assert_eq!(session.context_usage_percent(), 25.0);
    assert!(!session.is_context_exhausted());

    session.update_context(150_000);
    assert_eq!(session.context_usage_percent(), 75.0);
    assert!(session.is_context_exhausted());

    session
        .try_mark_context_exhausted()
        .expect("Running -> ContextExhausted");
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
fn test_multiple_sessions_independent() {
    let mut session1 = Session::new();
    let mut session2 = Session::new();
    let mut session3 = Session::new();

    session1.assign_to_stage("stage-1".to_string());
    session2.assign_to_stage("stage-2".to_string());
    session3.assign_to_stage("stage-3".to_string());

    session1
        .try_mark_running()
        .expect("s1: Spawning -> Running");
    session2
        .try_mark_running()
        .expect("s2: Spawning -> Running");
    session2.try_mark_paused().expect("s2: Running -> Paused");
    session3
        .try_mark_running()
        .expect("s3: Spawning -> Running");
    session3
        .try_mark_completed()
        .expect("s3: Running -> Completed");

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
