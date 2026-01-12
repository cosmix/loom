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
use loom::verify::transitions::{load_stage, save_stage, transition_stage, trigger_dependents};
use std::fs;
use tempfile::TempDir;

/// Helper function to create a test stage with specific parameters
fn create_test_stage(id: &str, name: &str, status: StageStatus) -> Stage {
    let mut stage = Stage::new(name.to_string(), Some(format!("Test stage {name}")));
    stage.id = id.to_string();
    stage.status = status;
    stage
}

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

    let triggered = trigger_dependents("stage-a", work_dir).unwrap();
    assert!(triggered.is_empty());

    let stage_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(stage_b.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_stage_reset_to_ready() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "test-stage".to_string();
    stage.status = StageStatus::Blocked;
    stage.add_dependency("dep-1".to_string());
    stage.add_dependency("dep-2".to_string());
    save_stage(&stage, work_dir).unwrap();

    // Blocked can only transition to Ready (per state machine)
    let reset = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(reset.status, StageStatus::Queued);
    assert_eq!(reset.dependencies.len(), 2);
    assert!(reset.dependencies.contains(&"dep-1".to_string()));
    assert!(reset.dependencies.contains(&"dep-2".to_string()));
}

#[test]
fn test_stage_reset_to_ready_when_deps_verified() {
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

    // Blocked can only transition to Ready (per state machine)
    let reset_ready = transition_stage("stage-b", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(reset_ready.status, StageStatus::Queued);

    // Stage B is already Ready, so triggering dependents won't add it again
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
    stage.add_acceptance_criterion("Must complete successfully".to_string());
    save_stage(&stage, work_dir).unwrap();

    // NeedsHandoff must first go to Ready, then Executing (per state machine)
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

    // Blocked must first go to Ready, then Executing (per state machine)
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

    let stage = transition_stage("test-stage", StageStatus::Completed, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
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

    // Blocked -> Ready (per state machine, Blocked can only go to Ready)
    let stage = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);

    // Ready -> Executing
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

    // NeedsHandoff -> Ready (per state machine, NeedsHandoff can only go to Ready)
    let stage = transition_stage("test-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);

    // Ready -> Executing
    let stage = transition_stage("test-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);

    // Executing -> Completed
    let stage = transition_stage("test-stage", StageStatus::Completed, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
}

#[test]
fn test_multiple_blocked_stages_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage_a = create_test_stage("stage-a", "Stage A", StageStatus::Blocked);
    save_stage(&stage_a, work_dir).unwrap();

    let mut stage_b = create_test_stage("stage-b", "Stage B", StageStatus::Blocked);
    stage_b.add_dependency("stage-a".to_string());
    save_stage(&stage_b, work_dir).unwrap();

    let mut stage_c = create_test_stage("stage-c", "Stage C", StageStatus::WaitingForDeps);
    stage_c.add_dependency("stage-b".to_string());
    save_stage(&stage_c, work_dir).unwrap();

    let triggered = trigger_dependents("stage-a", work_dir).unwrap();
    assert!(triggered.is_empty());

    let triggered = trigger_dependents("stage-b", work_dir).unwrap();
    assert!(triggered.is_empty());

    // Blocked -> Ready (per state machine)
    transition_stage("stage-a", StageStatus::Queued, work_dir).unwrap();
    transition_stage("stage-a", StageStatus::Executing, work_dir).unwrap();
    transition_stage("stage-a", StageStatus::Completed, work_dir).unwrap();
    transition_stage("stage-a", StageStatus::Completed, work_dir).unwrap();

    let triggered = trigger_dependents("stage-a", work_dir).unwrap();
    assert!(triggered.is_empty());

    let stage_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(stage_b.status, StageStatus::Blocked);
}

#[test]
fn test_blocked_stage_preserves_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new(
        "Complex Stage".to_string(),
        Some("A complex test stage".to_string()),
    );
    stage.id = "complex-stage".to_string();
    stage.status = StageStatus::Executing;
    stage.add_dependency("dep-1".to_string());
    stage.add_dependency("dep-2".to_string());
    stage.add_acceptance_criterion("Criterion A".to_string());
    stage.add_acceptance_criterion("Criterion B".to_string());
    stage.add_file_pattern("src/**/*.rs".to_string());
    stage.add_file_pattern("tests/**/*.rs".to_string());
    stage.set_parallel_group(Some("group-1".to_string()));

    let created_at = stage.created_at;
    save_stage(&stage, work_dir).unwrap();

    let blocked = transition_stage("complex-stage", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked.status, StageStatus::Blocked);
    assert_eq!(blocked.dependencies.len(), 2);
    assert_eq!(blocked.acceptance.len(), 2);
    assert_eq!(blocked.files.len(), 2);
    assert_eq!(blocked.parallel_group, Some("group-1".to_string()));
    assert_eq!(
        blocked.description,
        Some("A complex test stage".to_string())
    );
    assert_eq!(blocked.created_at, created_at);
    assert!(blocked.updated_at > created_at);
}

#[test]
fn test_needs_handoff_preserves_stage_context() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new(
        "Handoff Stage".to_string(),
        Some("Needs handoff due to context exhaustion".to_string()),
    );
    stage.id = "handoff-stage".to_string();
    stage.status = StageStatus::Executing;
    stage.add_acceptance_criterion("Must complete integration".to_string());
    stage.add_file_pattern("src/integration/**/*.rs".to_string());
    stage.set_worktree(Some("worktree-123".to_string()));
    stage.assign_session("session-abc".to_string());

    save_stage(&stage, work_dir).unwrap();

    let needs_handoff =
        transition_stage("handoff-stage", StageStatus::NeedsHandoff, work_dir).unwrap();
    assert_eq!(needs_handoff.status, StageStatus::NeedsHandoff);
    assert_eq!(needs_handoff.worktree, Some("worktree-123".to_string()));
    assert_eq!(needs_handoff.session, Some("session-abc".to_string()));
    assert_eq!(needs_handoff.acceptance.len(), 1);
    assert_eq!(needs_handoff.files.len(), 1);
}

#[test]
fn test_cascading_failure_does_not_propagate() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage_1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    save_stage(&stage_1, work_dir).unwrap();

    let mut stage_2 = create_test_stage("stage-2", "Stage 2", StageStatus::Executing);
    stage_2.add_dependency("stage-1".to_string());
    save_stage(&stage_2, work_dir).unwrap();

    let mut stage_3 = create_test_stage("stage-3", "Stage 3", StageStatus::WaitingForDeps);
    stage_3.add_dependency("stage-2".to_string());
    save_stage(&stage_3, work_dir).unwrap();

    let blocked_stage_2 = transition_stage("stage-2", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked_stage_2.status, StageStatus::Blocked);

    let triggered = trigger_dependents("stage-2", work_dir).unwrap();
    assert!(triggered.is_empty());

    let stage_3 = load_stage("stage-3", work_dir).unwrap();
    assert_eq!(stage_3.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_recovery_workflow_blocked_to_completion() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage = create_test_stage("recovery-stage", "Recovery Test", StageStatus::Executing);
    save_stage(&stage, work_dir).unwrap();

    let blocked = transition_stage("recovery-stage", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked.status, StageStatus::Blocked);

    // Blocked -> Ready (per state machine, Blocked can only go to Ready)
    let ready = transition_stage("recovery-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(ready.status, StageStatus::Queued);

    let executing = transition_stage("recovery-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(executing.status, StageStatus::Executing);

    let completed = transition_stage("recovery-stage", StageStatus::Completed, work_dir).unwrap();
    assert_eq!(completed.status, StageStatus::Completed);
    assert!(completed.completed_at.is_some());

    let verified = transition_stage("recovery-stage", StageStatus::Completed, work_dir).unwrap();
    assert_eq!(verified.status, StageStatus::Completed);
}

#[test]
fn test_parallel_stages_one_blocked_one_succeeds() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Stage 1 must be Completed AND merged for dependents to be triggered
    let mut stage_1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage_1.merged = true;
    save_stage(&stage_1, work_dir).unwrap();

    let mut stage_2a = create_test_stage("stage-2a", "Stage 2A", StageStatus::WaitingForDeps);
    stage_2a.add_dependency("stage-1".to_string());
    save_stage(&stage_2a, work_dir).unwrap();

    let mut stage_2b = create_test_stage("stage-2b", "Stage 2B", StageStatus::WaitingForDeps);
    stage_2b.add_dependency("stage-1".to_string());
    save_stage(&stage_2b, work_dir).unwrap();

    let mut stage_3 = create_test_stage("stage-3", "Stage 3", StageStatus::WaitingForDeps);
    stage_3.add_dependency("stage-2a".to_string());
    stage_3.add_dependency("stage-2b".to_string());
    save_stage(&stage_3, work_dir).unwrap();

    let triggered = trigger_dependents("stage-1", work_dir).unwrap();
    assert_eq!(triggered.len(), 2);

    transition_stage("stage-2a", StageStatus::Executing, work_dir).unwrap();
    let blocked_2a = transition_stage("stage-2a", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked_2a.status, StageStatus::Blocked);

    transition_stage("stage-2b", StageStatus::Executing, work_dir).unwrap();
    transition_stage("stage-2b", StageStatus::Completed, work_dir).unwrap();
    transition_stage("stage-2b", StageStatus::Completed, work_dir).unwrap();

    let triggered = trigger_dependents("stage-2b", work_dir).unwrap();
    assert!(triggered.is_empty());

    let stage_3 = load_stage("stage-3", work_dir).unwrap();
    assert_eq!(stage_3.status, StageStatus::WaitingForDeps);
}
