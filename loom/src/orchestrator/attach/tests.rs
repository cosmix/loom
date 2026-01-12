//! Tests for session attachment functionality.

use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::BackendType;

use super::helpers::format_status;
use super::list::format_attachable_list;
use super::loaders::{detect_backend_type, is_attachable};
use super::single::print_attach_instructions;
use super::types::{AttachableSession, SessionBackend};

use super::helpers::format_manual_mode_error;

#[test]
fn test_format_attachable_list() {
    let sessions = vec![
        AttachableSession {
            session_id: "session-1".to_string(),
            stage_id: Some("stage-1".to_string()),
            stage_name: Some("models".to_string()),
            backend: SessionBackend::Native { pid: 12345 },
            status: SessionStatus::Running,
            context_percent: 45.0,
        },
        AttachableSession {
            session_id: "session-2".to_string(),
            stage_id: Some("stage-2".to_string()),
            stage_name: Some("api".to_string()),
            backend: SessionBackend::Native { pid: 12346 },
            status: SessionStatus::Paused,
            context_percent: 23.5,
        },
    ];

    let output = format_attachable_list(&sessions);

    assert!(output.contains("SESSION"));
    assert!(output.contains("STAGE"));
    assert!(output.contains("BACKEND"));
    assert!(output.contains("STATUS"));
    assert!(output.contains("CONTEXT"));
    assert!(output.contains("session-1"));
    assert!(output.contains("session-2"));
    assert!(output.contains("models"));
    assert!(output.contains("api"));
    assert!(output.contains("native"));
    assert!(output.contains("running"));
    assert!(output.contains("paused"));
    assert!(output.contains("45%"));
    assert!(output.contains("24%"));
}

#[test]
fn test_format_attachable_list_long_names() {
    let sessions = vec![AttachableSession {
        session_id: "very-long-session-identifier-name".to_string(),
        stage_id: Some("stage-1".to_string()),
        stage_name: Some("very-long-stage-name-that-exceeds-limit".to_string()),
        backend: SessionBackend::Native { pid: 12345 },
        status: SessionStatus::Running,
        context_percent: 75.8,
    }];

    let output = format_attachable_list(&sessions);

    assert!(output.contains("very-long-ses..."));
    assert!(output.contains("very-long-stage..."));
    assert!(output.contains("76%"));
}

#[test]
fn test_print_attach_instructions() {
    print_attach_instructions("test-session");
}

#[test]
fn test_context_percent_calculation() {
    let session = AttachableSession {
        session_id: "test".to_string(),
        stage_id: None,
        stage_name: None,
        backend: SessionBackend::Native { pid: 12345 },
        status: SessionStatus::Running,
        context_percent: 75.5,
    };

    assert_eq!(session.context_percent, 75.5);
}

#[test]
fn test_attachable_filter() {
    let mut running_session = Session::new();
    running_session.status = SessionStatus::Running;
    running_session.pid = Some(12345);

    let mut paused_session = Session::new();
    paused_session.status = SessionStatus::Paused;
    paused_session.pid = Some(12346);

    let mut completed_session = Session::new();
    completed_session.status = SessionStatus::Completed;
    completed_session.pid = Some(12347);

    let mut spawning_session = Session::new();
    spawning_session.status = SessionStatus::Spawning;
    spawning_session.pid = Some(12348);

    let mut no_backend_session = Session::new();
    no_backend_session.status = SessionStatus::Running;
    no_backend_session.pid = None;

    assert!(is_attachable(&running_session));
    assert!(is_attachable(&paused_session));
    assert!(!is_attachable(&completed_session));
    assert!(!is_attachable(&spawning_session));
    assert!(!is_attachable(&no_backend_session));
}

#[test]
fn test_attachable_filter_native() {
    let mut running_native = Session::new();
    running_native.status = SessionStatus::Running;
    running_native.pid = Some(12345);

    let mut paused_native = Session::new();
    paused_native.status = SessionStatus::Paused;
    paused_native.pid = Some(12346);

    let mut completed_native = Session::new();
    completed_native.status = SessionStatus::Completed;
    completed_native.pid = Some(12347);

    assert!(is_attachable(&running_native));
    assert!(is_attachable(&paused_native));
    assert!(!is_attachable(&completed_native));
}

#[test]
fn test_detect_backend_type() {
    let mut native_session = Session::new();
    native_session.pid = Some(12345);
    assert_eq!(
        detect_backend_type(&native_session),
        Some(BackendType::Native)
    );

    let no_backend = Session::new();
    assert_eq!(detect_backend_type(&no_backend), None);
}

#[test]
fn test_session_backend_methods() {
    let native_session = AttachableSession {
        session_id: "test".to_string(),
        stage_id: None,
        stage_name: None,
        backend: SessionBackend::Native { pid: 12345 },
        status: SessionStatus::Running,
        context_percent: 50.0,
    };

    assert!(native_session.is_native());
    assert_eq!(native_session.pid(), 12345);
    assert_eq!(native_session.backend_type(), BackendType::Native);
}

#[test]
fn test_format_status() {
    assert_eq!(format_status(&SessionStatus::Running), "running");
    assert_eq!(format_status(&SessionStatus::Paused), "paused");
    assert_eq!(format_status(&SessionStatus::Completed), "completed");
    assert_eq!(format_status(&SessionStatus::Crashed), "crashed");
    assert_eq!(format_status(&SessionStatus::ContextExhausted), "exhausted");
    assert_eq!(format_status(&SessionStatus::Spawning), "spawning");
}

#[test]
fn test_format_manual_mode_error_with_worktree() {
    let work_dir = std::path::Path::new("/project/.work");
    let worktree_path = std::path::PathBuf::from("/project/.worktrees/stage-1");
    let error = format_manual_mode_error("session-123", Some(&worktree_path), work_dir);

    let error_msg = error.to_string();
    assert!(error_msg.contains("session-123"));
    assert!(error_msg.contains("manual mode"));
    assert!(error_msg.contains("cd /project/.worktrees/stage-1"));
    assert!(error_msg.contains("signals/session-123.md"));
}

#[test]
fn test_format_manual_mode_error_without_worktree() {
    let work_dir = std::path::Path::new("/project/.work");
    let error = format_manual_mode_error("session-456", None, work_dir);

    let error_msg = error.to_string();
    assert!(error_msg.contains("session-456"));
    assert!(error_msg.contains("manual mode"));
    assert!(error_msg.contains("cd .worktrees/<stage-id>"));
    assert!(error_msg.contains("signals/session-456.md"));
}

#[test]
fn test_print_attach_instructions_long_name() {
    // Should not panic with a very long session name
    print_attach_instructions("this-is-a-very-long-session-name-that-exceeds-32-characters");
}

#[test]
fn test_format_attachable_list_native_sessions() {
    let sessions = vec![
        AttachableSession {
            session_id: "session-1".to_string(),
            stage_id: Some("stage-1".to_string()),
            stage_name: Some("models".to_string()),
            backend: SessionBackend::Native { pid: 12345 },
            status: SessionStatus::Running,
            context_percent: 45.0,
        },
        AttachableSession {
            session_id: "session-2".to_string(),
            stage_id: Some("stage-2".to_string()),
            stage_name: Some("api".to_string()),
            backend: SessionBackend::Native { pid: 12346 },
            status: SessionStatus::Running,
            context_percent: 30.0,
        },
    ];

    let output = format_attachable_list(&sessions);

    assert!(output.contains("BACKEND"));
    assert!(output.contains("native"));
}
