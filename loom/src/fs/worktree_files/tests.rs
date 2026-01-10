//! Tests for worktree file operations

use super::*;
use std::fs;
use tempfile::TempDir;

fn setup_work_dir() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create subdirectories
    fs::create_dir_all(work_dir.join("sessions")).unwrap();
    fs::create_dir_all(work_dir.join("signals")).unwrap();
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::create_dir_all(work_dir.join("archive")).unwrap();

    temp_dir
}

fn create_session_file(work_dir: &std::path::Path, session_id: &str, stage_id: &str) {
    let content = format!(
        r#"---
id: {session_id}
stage_id: {stage_id}
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session: {session_id}
"#
    );
    fs::write(
        work_dir.join("sessions").join(format!("{session_id}.md")),
        content,
    )
    .unwrap();
}

fn create_signal_file(work_dir: &std::path::Path, session_id: &str) {
    let content = format!("# Signal: {session_id}\n");
    fs::write(
        work_dir.join("signals").join(format!("{session_id}.md")),
        content,
    )
    .unwrap();
}

fn create_stage_file(work_dir: &std::path::Path, stage_id: &str) {
    let content = format!(
        r#"---
id: {stage_id}
name: Test Stage
status: Verified
---

# Stage: {stage_id}
"#
    );
    fs::write(
        work_dir.join("stages").join(format!("01-{stage_id}.md")),
        content,
    )
    .unwrap();
}

#[test]
fn test_cleanup_config_default() {
    let config = StageFileCleanupConfig::default();
    assert!(config.cleanup_sessions);
    assert!(config.cleanup_signals);
    assert!(!config.archive_stage);
    assert!(config.verbose);
}

#[test]
fn test_cleanup_config_quiet() {
    let config = StageFileCleanupConfig::quiet();
    assert!(!config.verbose);
}

#[test]
fn test_cleanup_config_with_archive() {
    let config = StageFileCleanupConfig::with_archive();
    assert!(config.archive_stage);
}

#[test]
fn test_cleanup_result_any_cleanup_done() {
    let mut result = StageFileCleanupResult::default();
    assert!(!result.any_cleanup_done());

    result.sessions_removed = 1;
    assert!(result.any_cleanup_done());
}

#[test]
fn test_find_sessions_for_stage_empty() {
    let temp_dir = setup_work_dir();
    let result = find_sessions_for_stage("stage-1", temp_dir.path());
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_find_sessions_for_stage_found() {
    let temp_dir = setup_work_dir();
    create_session_file(temp_dir.path(), "session-1", "stage-1");
    create_session_file(temp_dir.path(), "session-2", "stage-1");
    create_session_file(temp_dir.path(), "session-3", "other-stage");

    let result = find_sessions_for_stage("stage-1", temp_dir.path());
    assert!(result.is_ok());
    let sessions = result.unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.contains(&"session-1".to_string()));
    assert!(sessions.contains(&"session-2".to_string()));
}

#[test]
fn test_remove_signal_file_exists() {
    let temp_dir = setup_work_dir();
    create_signal_file(temp_dir.path(), "session-1");

    let result = remove_signal_file("session-1", temp_dir.path());
    assert!(result.is_ok());
    assert!(result.unwrap());
    assert!(!temp_dir.path().join("signals/session-1.md").exists());
}

#[test]
fn test_remove_signal_file_not_exists() {
    let temp_dir = setup_work_dir();

    let result = remove_signal_file("nonexistent", temp_dir.path());
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_remove_session_file_exists() {
    let temp_dir = setup_work_dir();
    create_session_file(temp_dir.path(), "session-1", "stage-1");

    let result = remove_session_file("session-1", temp_dir.path());
    assert!(result.is_ok());
    assert!(result.unwrap());
    assert!(!temp_dir.path().join("sessions/session-1.md").exists());
}

#[test]
fn test_stage_has_files_empty() {
    let temp_dir = setup_work_dir();
    assert!(!stage_has_files("stage-1", temp_dir.path()));
}

#[test]
fn test_stage_has_files_with_session() {
    let temp_dir = setup_work_dir();
    create_session_file(temp_dir.path(), "session-1", "stage-1");
    assert!(stage_has_files("stage-1", temp_dir.path()));
}

#[test]
fn test_stage_has_files_with_stage_file() {
    let temp_dir = setup_work_dir();
    create_stage_file(temp_dir.path(), "stage-1");
    assert!(stage_has_files("stage-1", temp_dir.path()));
}

#[test]
fn test_cleanup_stage_files_complete() {
    let temp_dir = setup_work_dir();

    // Set up files for stage-1
    create_session_file(temp_dir.path(), "session-1", "stage-1");
    create_session_file(temp_dir.path(), "session-2", "stage-1");
    create_signal_file(temp_dir.path(), "session-1");
    create_signal_file(temp_dir.path(), "session-2");
    create_stage_file(temp_dir.path(), "stage-1");

    // Also create files for another stage (should not be cleaned)
    create_session_file(temp_dir.path(), "session-3", "other-stage");

    let config = StageFileCleanupConfig::quiet();
    let result = cleanup_stage_files("stage-1", temp_dir.path(), &config);

    assert!(result.is_ok());
    let cleanup_result = result.unwrap();
    assert_eq!(cleanup_result.sessions_removed, 2);
    assert_eq!(cleanup_result.signals_removed, 2);
    assert!(cleanup_result.any_cleanup_done());

    // Verify files are gone
    assert!(!temp_dir.path().join("sessions/session-1.md").exists());
    assert!(!temp_dir.path().join("sessions/session-2.md").exists());
    assert!(!temp_dir.path().join("signals/session-1.md").exists());
    assert!(!temp_dir.path().join("signals/session-2.md").exists());

    // Verify other stage files remain
    assert!(temp_dir.path().join("sessions/session-3.md").exists());
}

#[test]
fn test_cleanup_stage_files_with_archive() {
    let temp_dir = setup_work_dir();
    create_stage_file(temp_dir.path(), "stage-1");

    let config = StageFileCleanupConfig::with_archive();
    let result = cleanup_stage_files("stage-1", temp_dir.path(), &config);

    assert!(result.is_ok());
    let cleanup_result = result.unwrap();
    assert!(cleanup_result.stage_file_handled);

    // Verify file was moved to archive
    assert!(!temp_dir.path().join("stages/01-stage-1.md").exists());
    assert!(temp_dir.path().join("archive/01-stage-1.md").exists());
}

#[test]
fn test_find_stage_file_by_id_with_prefix() {
    let temp_dir = setup_work_dir();
    create_stage_file(temp_dir.path(), "my-stage");

    let result = find_stage_file_by_id(&temp_dir.path().join("stages"), "my-stage");
    assert!(result.is_ok());
    let path = result.unwrap();
    assert!(path.is_some());
    assert!(path.unwrap().ends_with("01-my-stage.md"));
}

#[test]
fn test_find_stage_file_by_id_not_found() {
    let temp_dir = setup_work_dir();

    let result = find_stage_file_by_id(&temp_dir.path().join("stages"), "nonexistent");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}
