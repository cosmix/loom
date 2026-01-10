//! Tests for stage commands

use super::complete::cleanup_terminal_for_stage;
use super::session::{find_session_for_stage, session_from_markdown, session_to_markdown};
use super::state::{block, hold, ready, release, reset, resume_from_waiting, waiting};
use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::load_stage;
use chrono::Utc;
use serial_test::serial;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn create_test_stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Stage {id}"),
        description: None,
        status,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        plan_id: None,
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
        close_reason: None,
        auto_merge: None,
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
    }
}

fn setup_work_dir() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();
    temp_dir
}

fn save_test_stage(work_dir: &Path, stage: &Stage) {
    let yaml = serde_yaml::to_string(stage).unwrap();
    let content = format!("---\n{yaml}---\n\n# Stage: {}\n", stage.name);

    let stages_dir = work_dir.join("stages");
    fs::create_dir_all(&stages_dir).unwrap();

    let stage_path = stages_dir.join(format!("00-{}.md", stage.id));
    fs::write(stage_path, content).unwrap();
}

#[test]
fn test_session_from_markdown_valid() {
    let content = r#"---
id: session-1
stage_id: stage-1
tmux_session: null
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
        tmux_session: Some("loom-stage-1".to_string()),
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

    let content = session_to_markdown(&session);

    assert!(content.starts_with("---\n"));
    assert!(content.contains("# Session: session-1"));
    assert!(content.contains("**Status**: Running"));
    assert!(content.contains("**Stage**: stage-1"));
}

#[test]
fn test_find_session_for_stage_found() {
    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".work");

    let session_content = r#"---
id: session-1
stage_id: test-stage
tmux_session: null
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
tmux_session: null
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
fn test_cleanup_tmux_for_stage_does_not_fail() {
    cleanup_terminal_for_stage("test-stage", None, Path::new(".work"));
}

#[test]
fn test_cleanup_session_resources() {
    use super::session::cleanup_session_resources;

    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".work");

    let session = Session {
        id: "session-1".to_string(),
        stage_id: Some("test-stage".to_string()),
        tmux_session: None,
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

#[test]
#[serial]
fn test_block_updates_status() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let stage = create_test_stage("test-stage", StageStatus::Queued);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = block("test-stage".to_string(), "Test blocker".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "block() failed: {:?}", result.err());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Blocked);
    assert_eq!(loaded_stage.close_reason, Some("Test blocker".to_string()));
}

#[test]
#[serial]
fn test_reset_clears_completion() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let mut stage = create_test_stage("test-stage", StageStatus::Completed);
    stage.completed_at = Some(Utc::now());
    stage.close_reason = Some("Done".to_string());
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = reset("test-stage".to_string(), false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "reset() failed: {:?}", result.err());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::WaitingForDeps);
    assert_eq!(loaded_stage.completed_at, None);
    assert_eq!(loaded_stage.close_reason, None);
}

#[test]
#[serial]
fn test_reset_hard_clears_session() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.session = Some("session-1".to_string());
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = reset("test-stage".to_string(), true, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.session, None);
}

#[test]
#[serial]
fn test_ready_marks_as_ready() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let stage = create_test_stage("test-stage", StageStatus::WaitingForDeps);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = ready("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Queued);
}

#[test]
#[serial]
fn test_hold_sets_held_flag() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let stage = create_test_stage("test-stage", StageStatus::Queued);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = hold("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert!(loaded_stage.held);
}

#[test]
#[serial]
fn test_release_clears_held_flag() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let mut stage = create_test_stage("test-stage", StageStatus::Queued);
    stage.held = true;
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = release("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert!(!loaded_stage.held);
}

#[test]
#[serial]
fn test_waiting_transitions_from_executing() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let stage = create_test_stage("test-stage", StageStatus::Executing);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = waiting("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::WaitingForInput);
}

#[test]
#[serial]
fn test_waiting_skips_if_not_executing() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let stage = create_test_stage("test-stage", StageStatus::Queued);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = waiting("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Queued);
}

#[test]
#[serial]
fn test_resume_from_waiting_transitions_to_executing() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let stage = create_test_stage("test-stage", StageStatus::WaitingForInput);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = resume_from_waiting("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Executing);
}

#[test]
#[serial]
fn test_complete_with_passing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec!["exit 0".to_string()];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("test-stage".to_string(), None, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    // After refactor: complete goes directly to Completed (no more Verified)
    assert_eq!(loaded_stage.status, StageStatus::Completed);
}

#[test]
#[serial]
fn test_complete_with_no_verify_flag() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec!["exit 1".to_string()];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("test-stage".to_string(), None, true);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
}
