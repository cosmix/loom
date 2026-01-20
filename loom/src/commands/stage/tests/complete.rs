//! Tests for complete command

use super::super::complete::complete;
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::load_stage;
use serial_test::serial;

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

    let result = complete("test-stage".to_string(), None, false, false, false);

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

    let result = complete("test-stage".to_string(), None, true, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_sets_merged_true() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage (no acceptance criteria)
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
    // Key assertion: merged=true is auto-set for knowledge stages
    assert!(
        loaded_stage.merged,
        "Knowledge stage should auto-set merged=true"
    );
}

#[test]
#[serial]
fn test_complete_knowledge_stage_with_passing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage with passing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec!["exit 0".to_string()];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
    assert!(loaded_stage.merged);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_with_failing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage with failing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec!["exit 1".to_string()];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(
        result.is_ok(),
        "complete() should succeed even with failed acceptance"
    );

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    // Failing acceptance results in CompletedWithFailures
    assert_eq!(loaded_stage.status, StageStatus::CompletedWithFailures);
    // merged should NOT be set when acceptance fails
    assert!(!loaded_stage.merged);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_triggers_dependents() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage
    let mut knowledge_stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    knowledge_stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &knowledge_stage);

    // Create a dependent stage waiting for the knowledge stage
    let mut dependent_stage = create_test_stage("dependent-stage", StageStatus::WaitingForDeps);
    dependent_stage.dependencies = vec!["knowledge-stage".to_string()];
    save_test_stage(&work_dir_path, &dependent_stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    // Verify knowledge stage is completed with merged=true
    let loaded_knowledge = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_knowledge.status, StageStatus::Completed);
    assert!(loaded_knowledge.merged);

    // Verify dependent stage was triggered to Queued
    let loaded_dependent = load_stage("dependent-stage", &work_dir_path).unwrap();
    assert_eq!(
        loaded_dependent.status,
        StageStatus::Queued,
        "Dependent stage should be triggered to Queued when knowledge stage completes with merged=true"
    );
}

#[test]
#[serial]
fn test_complete_standard_stage_not_routed_to_knowledge() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a standard stage (default stage_type)
    let mut stage = create_test_stage("standard-stage", StageStatus::Executing);
    stage.acceptance = vec!["exit 0".to_string()];
    // Ensure it's explicitly standard (default)
    stage.stage_type = StageType::Standard;
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("standard-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    // Standard stages go through the normal completion path
    // which attempts merge (and succeeds in this test setup since no worktree)
    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("standard-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
}
