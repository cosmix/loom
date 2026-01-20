//! Tests for state transition validation

use tempfile::TempDir;

use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, transition_stage};

use super::create_test_stage;

#[test]
fn test_transition_stage_to_ready() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
    save_stage(&stage, work_dir).expect("Should save stage");

    let updated = transition_stage("stage-1", StageStatus::Queued, work_dir)
        .expect("Should transition stage");

    assert_eq!(updated.status, StageStatus::Queued);

    let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
    assert_eq!(reloaded.status, StageStatus::Queued);
}

#[test]
fn test_transition_stage_to_completed() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
    save_stage(&stage, work_dir).expect("Should save stage");

    let updated = transition_stage("stage-1", StageStatus::Completed, work_dir)
        .expect("Should transition stage");

    assert_eq!(updated.status, StageStatus::Completed);
}

#[test]
fn test_transition_stage_invalid_completed_to_pending() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Completed);
    save_stage(&stage, work_dir).expect("Should save stage");

    let result = transition_stage("stage-1", StageStatus::WaitingForDeps, work_dir);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Invalid") || err.contains("transition"),
        "Error should mention invalid transition: {err}"
    );

    // Verify stage status was not changed
    let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
    assert_eq!(
        reloaded.status,
        StageStatus::Completed,
        "Stage should remain in Completed status"
    );
}

#[test]
fn test_transition_stage_invalid_pending_to_completed() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
    save_stage(&stage, work_dir).expect("Should save stage");

    let result = transition_stage("stage-1", StageStatus::Completed, work_dir);
    assert!(result.is_err());

    // Verify stage status was not changed
    let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
    assert_eq!(reloaded.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_transition_stage_invalid_ready_to_completed() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Queued);
    save_stage(&stage, work_dir).expect("Should save stage");

    let result = transition_stage("stage-1", StageStatus::Completed, work_dir);
    assert!(result.is_err());

    // Verify stage status was not changed
    let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
    assert_eq!(reloaded.status, StageStatus::Queued);
}

#[test]
fn test_transition_stage_invalid_completed_to_executing() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Completed);
    save_stage(&stage, work_dir).expect("Should save stage");

    let result = transition_stage("stage-1", StageStatus::Executing, work_dir);
    assert!(result.is_err());

    // Verify stage status was not changed
    let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
    assert_eq!(reloaded.status, StageStatus::Completed);
}

#[test]
fn test_transition_stage_valid_full_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stage in Pending status
    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
    save_stage(&stage, work_dir).expect("Should save stage");

    // Pending -> Ready (valid)
    let updated =
        transition_stage("stage-1", StageStatus::Queued, work_dir).expect("Pending->Ready");
    assert_eq!(updated.status, StageStatus::Queued);

    // Ready -> Executing (valid)
    let updated =
        transition_stage("stage-1", StageStatus::Executing, work_dir).expect("Ready->Executing");
    assert_eq!(updated.status, StageStatus::Executing);

    // Executing -> Completed (valid, terminal state)
    let updated = transition_stage("stage-1", StageStatus::Completed, work_dir)
        .expect("Executing->Completed");
    assert_eq!(updated.status, StageStatus::Completed);

    // Completed is terminal, no further transitions allowed
    let result = transition_stage("stage-1", StageStatus::Queued, work_dir);
    assert!(result.is_err());
}

#[test]
fn test_transition_stage_same_status_is_valid() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
    save_stage(&stage, work_dir).expect("Should save stage");

    // Same status transition should succeed (no-op)
    let updated = transition_stage("stage-1", StageStatus::Executing, work_dir)
        .expect("Same status should be valid");
    assert_eq!(updated.status, StageStatus::Executing);
}

#[test]
fn test_transition_stage_blocked_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stage in Executing status
    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
    save_stage(&stage, work_dir).expect("Should save stage");

    // Executing -> Blocked (valid)
    let updated =
        transition_stage("stage-1", StageStatus::Blocked, work_dir).expect("Executing->Blocked");
    assert_eq!(updated.status, StageStatus::Blocked);

    // Blocked -> Ready (valid - unblocking)
    let updated =
        transition_stage("stage-1", StageStatus::Queued, work_dir).expect("Blocked->Ready");
    assert_eq!(updated.status, StageStatus::Queued);
}

#[test]
fn test_transition_stage_handoff_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stage in Executing status
    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
    save_stage(&stage, work_dir).expect("Should save stage");

    // Executing -> NeedsHandoff (valid)
    let updated = transition_stage("stage-1", StageStatus::NeedsHandoff, work_dir)
        .expect("Executing->NeedsHandoff");
    assert_eq!(updated.status, StageStatus::NeedsHandoff);

    // NeedsHandoff -> Ready (valid - resuming)
    let updated =
        transition_stage("stage-1", StageStatus::Queued, work_dir).expect("NeedsHandoff->Ready");
    assert_eq!(updated.status, StageStatus::Queued);
}

#[test]
fn test_transition_stage_waiting_for_input() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stage in Executing status
    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
    save_stage(&stage, work_dir).expect("Should save stage");

    // Executing -> WaitingForInput (valid)
    let updated = transition_stage("stage-1", StageStatus::WaitingForInput, work_dir)
        .expect("Executing->WaitingForInput");
    assert_eq!(updated.status, StageStatus::WaitingForInput);

    // WaitingForInput -> Executing (valid - input received)
    let updated = transition_stage("stage-1", StageStatus::Executing, work_dir)
        .expect("WaitingForInput->Executing");
    assert_eq!(updated.status, StageStatus::Executing);
}
