//! Tests for state transition commands

use super::super::state::{block, hold, ready, release, reset, resume_from_waiting, waiting};
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::models::stage::StageStatus;
use crate::verify::transitions::load_stage;
use chrono::Utc;
use serial_test::serial;

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
