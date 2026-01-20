//! Tests for session operations

use super::super::complete::cleanup_terminal_for_stage;
use super::super::session::{
    cleanup_session_resources, find_session_for_stage, session_from_markdown, session_to_markdown,
};
use super::setup_work_dir;
use crate::models::session::{Session, SessionStatus, SessionType};
use chrono::Utc;
use std::fs;
use std::path::Path;

#[test]
fn test_session_from_markdown_valid() {
    let content = r#"---
id: session-1
stage_id: stage-1
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session: session-1
"#;

    let result = session_from_markdown(content);

    assert!(result.is_ok());
    let session = result.unwrap();
    assert_eq!(session.id, "session-1");
    assert_eq!(session.stage_id, Some("stage-1".to_string()));
}

#[test]
fn test_session_from_markdown_no_frontmatter() {
    let content = "No frontmatter here";

    let result = session_from_markdown(content);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("missing frontmatter"));
}

#[test]
fn test_session_to_markdown() {
    let session = Session {
        id: "session-1".to_string(),
        stage_id: Some("stage-1".to_string()),
        worktree_path: None,
        pid: Some(12345),
        status: SessionStatus::Running,
        context_tokens: 0,
        context_limit: 200000,
        created_at: Utc::now(),
        last_active: Utc::now(),
        session_type: SessionType::default(),
        merge_source_branch: None,
        merge_target_branch: None,
    };

    let content = session_to_markdown(&session);

    assert!(content.starts_with("---\n"));
    assert!(content.contains("# Session: session-1"));
    assert!(content.contains("**Status**: Running"));
    assert!(content.contains("**Stage**: stage-1"));
    assert!(content.contains("**PID**: 12345"));
}

#[test]
fn test_find_session_for_stage_found() {
    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".work");

    let session_content = r#"---
id: session-1
stage_id: test-stage
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session
"#;

    let sessions_dir = work_dir.join("sessions");
    fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

    let result = find_session_for_stage("test-stage", &work_dir);

    assert_eq!(result, Some("session-1".to_string()));
}

#[test]
fn test_find_session_for_stage_not_found() {
    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".work");

    let result = find_session_for_stage("nonexistent", &work_dir);

    assert_eq!(result, None);
}

#[test]
fn test_find_session_for_stage_different_stage() {
    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".work");

    let session_content = r#"---
id: session-1
stage_id: other-stage
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---
"#;

    let sessions_dir = work_dir.join("sessions");
    fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

    let result = find_session_for_stage("test-stage", &work_dir);

    assert_eq!(result, None);
}

#[test]
fn test_cleanup_terminal_for_stage_does_not_fail() {
    cleanup_terminal_for_stage("test-stage", None, Path::new(".work"));
}

#[test]
fn test_cleanup_session_resources() {
    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".work");

    let session = Session {
        id: "session-1".to_string(),
        stage_id: Some("test-stage".to_string()),
        worktree_path: None,
        pid: None,
        status: SessionStatus::Running,
        context_tokens: 0,
        context_limit: 200000,
        created_at: Utc::now(),
        last_active: Utc::now(),
        session_type: SessionType::default(),
        merge_source_branch: None,
        merge_target_branch: None,
    };

    let session_content = session_to_markdown(&session);
    let sessions_dir = work_dir.join("sessions");
    fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

    let signals_dir = work_dir.join("signals");
    fs::write(signals_dir.join("session-1.md"), "signal").unwrap();

    cleanup_session_resources("test-stage", "session-1", &work_dir);

    assert!(!signals_dir.join("session-1.md").exists());
}
