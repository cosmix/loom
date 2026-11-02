//! End-to-end tests for stage failure handling and resume functionality
//!
//! This test suite covers:
//! - Blocking and unblocking stages
//! - Stage reset scenarios
//! - Dependency triggering with blocked stages
//! - Session crash handling
//! - Resume from handoff
//! - Valid state machine transitions

use loom::models::stage::{Stage, StageStatus};
use loom::plan::schema::AcceptanceCriterion;
use loom::verify::transitions::{load_stage, save_stage, transition_stage, trigger_dependents};
use std::fs;
use tempfile::TempDir;

#[path = "stage_fixture/mod.rs"]
mod stage_fixture;

use stage_fixture::knowledge_stage as create_test_stage;

#[test]
fn test_stage_can_be_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::Executing;
    stage.add_dependency("other-stage".to_string());
    save_stage(&stage, work_dir).unwrap();

    let blocked = transition_stage("test-stage", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked.status, StageStatus::Blocked);
    assert!(blocked.dependencies.contains(&"other-stage".to_string()));

    let reloaded = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::Blocked);
    assert_eq!(reloaded.dependencies.len(), 1);
}

#[test]
fn test_blocked_stage_does_not_trigger_dependents() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage_a = Stage::new("Stage A".to_string(), None);
    stage_a.id = "stage-a".to_string();
    stage_a.status = StageStatus::Blocked;
    save_stage(&stage_a, work_dir).unwrap();

    let mut stage_b = Stage::new("Stage B".to_string(), None);
    stage_b.id = "stage-b".to_string();
    stage_b.status = StageStatus::WaitingForDeps;
    stage_b.add_dependency("stage-a".to_string());
    save_stage(&stage_b, work_dir).unwrap();

    let triggered = trigger_dependents("stage-a", work_dir, work_dir, "main").unwrap();
    assert!(triggered.is_empty());

    let stage_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(stage_b.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_blocked_stage_requeues_preserving_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::Blocked;
    stage.add_dependency("dep-1".to_string());
    stage.add_dependency("dep-2".to_string());
    save_stage(&stage, work_dir).unwrap();

    // Blocked can only transition to Queued (per state machine)
    let reset = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(reset.status, StageStatus::Queued);
    assert_eq!(reset.dependencies.len(), 2);
    assert!(reset.dependencies.contains(&"dep-1".to_string()));
    assert!(reset.dependencies.contains(&"dep-2".to_string()));
}

#[test]
fn test_blocked_stage_requeues_when_dependency_completed() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage_a = Stage::new("Stage A".to_string(), None);
    stage_a.id = "stage-a".to_string();
    stage_a.status = StageStatus::Completed;
    save_stage(&stage_a, work_dir).unwrap();

    let mut stage_b = Stage::new("Stage B".to_string(), None);
    stage_b.id = "stage-b".to_string();
    stage_b.status = StageStatus::Blocked;
    stage_b.add_dependency("stage-a".to_string());
    save_stage(&stage_b, work_dir).unwrap();

    // Blocked can only transition to Queued (per state machine)
    let reset_ready = transition_stage("stage-b", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(reset_ready.status, StageStatus::Queued);

    // Stage B is already Queued, so triggering dependents won't add it again
    let stage_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(stage_b.status, StageStatus::Queued);
}

#[test]
fn test_stage_needs_handoff_transition() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::Executing;
    save_stage(&stage, work_dir).unwrap();

    let needs_handoff =
        transition_stage("test-stage", StageStatus::NeedsHandoff, work_dir).unwrap();
    assert_eq!(needs_handoff.status, StageStatus::NeedsHandoff);

    let reloaded = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::NeedsHandoff);
}

#[test]
fn test_resume_from_needs_handoff_to_executing() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::NeedsHandoff;
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(
        "Must complete successfully".to_string(),
    ));
    save_stage(&stage, work_dir).unwrap();

    // NeedsHandoff must first go to Queued, then Executing (per state machine)
    let ready = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(ready.status, StageStatus::Queued);

    let resumed = transition_stage("test-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(resumed.status, StageStatus::Executing);
    assert_eq!(resumed.acceptance.len(), 1);

    let reloaded = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::Executing);
}

#[test]
fn test_resume_from_blocked_to_executing() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::Blocked;
    stage.add_file_pattern("src/**/*.rs".to_string());
    save_stage(&stage, work_dir).unwrap();

    // Blocked must first go to Queued, then Executing (per state machine)
    let ready = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(ready.status, StageStatus::Queued);

    let resumed = transition_stage("test-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(resumed.status, StageStatus::Executing);
    assert!(resumed.files.contains(&"src/**/*.rs".to_string()));

    let reloaded = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::Executing);
}

#[test]
fn test_stage_state_machine_valid_transitions() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();

    stage.status = StageStatus::WaitingForDeps;
    save_stage(&stage, work_dir).unwrap();
    let loaded = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(loaded.status, StageStatus::WaitingForDeps);

    let stage = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);

    let stage = transition_stage("test-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);

    let stage = transition_stage("test-stage", StageStatus::Completed, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.completed_at.is_some());
}

#[test]
fn test_stage_state_machine_blocked_path() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::Executing;
    save_stage(&stage, work_dir).unwrap();

    // Executing -> Blocked
    let stage = transition_stage("test-stage", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);

    // Blocked -> Queued (per state machine, Blocked can only go to Queued)
    let stage = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);

    // Queued -> Executing
    let stage = transition_stage("test-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn test_stage_state_machine_needs_handoff_path() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::Executing;
    save_stage(&stage, work_dir).unwrap();

    // Executing -> NeedsHandoff
    let stage = transition_stage("test-stage", StageStatus::NeedsHandoff, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHandoff);

    // NeedsHandoff -> Queued (per state machine, NeedsHandoff can only go to Queued)
    let stage = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);

    // Queued -> Executing
    let stage = transition_stage("test-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);

    // Executing -> Completed
    let stage = transition_stage("test-stage", StageStatus::Completed, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
}

#[path = "failure_resume/workflows.rs"]
mod workflows;
