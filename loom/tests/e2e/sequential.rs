//! E2E tests for sequential plan execution
//!
//! These tests verify that sequential plans (stages with linear dependencies)
//! execute in the correct order and respect dependency constraints.

use super::helpers::complete_stage;
use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::{
    list_all_stages, load_stage, save_stage, transition_stage, trigger_dependents,
};
use tempfile::TempDir;

/// Test that a sequential 3-stage plan maintains correct stage order
///
/// Creates: A -> B -> C
/// Verifies:
/// - Stage 1 starts Ready (no deps)
/// - Stages 2 and 3 start Pending
/// - After stage 1 completes, stage 2 becomes Ready while stage 3 remains Pending
#[test]
fn test_sequential_plan_stage_order() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create stage 1 (no dependencies)
    let mut stage1 = Stage::new("Stage 1".to_string(), Some("First stage".to_string()));
    stage1.id = "stage-1".to_string();
    stage1.status = StageStatus::Queued;
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    // Create stage 2 (depends on stage 1)
    let mut stage2 = Stage::new("Stage 2".to_string(), Some("Second stage".to_string()));
    stage2.id = "stage-2".to_string();
    stage2.status = StageStatus::WaitingForDeps;
    stage2.add_dependency("stage-1".to_string());
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    // Create stage 3 (depends on stage 2)
    let mut stage3 = Stage::new("Stage 3".to_string(), Some("Third stage".to_string()));
    stage3.id = "stage-3".to_string();
    stage3.status = StageStatus::WaitingForDeps;
    stage3.add_dependency("stage-2".to_string());
    save_stage(&stage3, work_dir).expect("Should save stage 3");

    // Verify initial states
    let loaded_stage1 = load_stage("stage-1", work_dir).expect("Should load stage 1");
    assert_eq!(loaded_stage1.status, StageStatus::Queued);

    let loaded_stage2 = load_stage("stage-2", work_dir).expect("Should load stage 2");
    assert_eq!(loaded_stage2.status, StageStatus::WaitingForDeps);

    let loaded_stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(loaded_stage3.status, StageStatus::WaitingForDeps);

    // Complete and verify stage 1
    complete_stage("stage-1", work_dir).expect("Should complete stage 1");
    let verified_stage1 = transition_stage("stage-1", StageStatus::Completed, work_dir)
        .expect("Should verify stage 1");
    assert_eq!(verified_stage1.status, StageStatus::Completed);

    // Trigger dependents
    let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-2");

    // Verify stage 2 is now Ready
    let loaded_stage2 = load_stage("stage-2", work_dir).expect("Should load stage 2");
    assert_eq!(loaded_stage2.status, StageStatus::Queued);

    // Verify stage 3 is still Pending
    let loaded_stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(loaded_stage3.status, StageStatus::WaitingForDeps);
}

/// Test full completion of a sequential plan
///
/// Runs through all stages sequentially (A -> B -> C) and verifies:
/// - Each dependent only becomes Ready after its dependency is Verified
/// - Final state: all stages Verified
#[test]
fn test_sequential_plan_full_completion() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create stage 1 (no dependencies)
    let mut stage1 = Stage::new("Stage 1".to_string(), Some("First stage".to_string()));
    stage1.id = "stage-1".to_string();
    stage1.status = StageStatus::Queued;
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    // Create stage 2 (depends on stage 1)
    let mut stage2 = Stage::new("Stage 2".to_string(), Some("Second stage".to_string()));
    stage2.id = "stage-2".to_string();
    stage2.status = StageStatus::WaitingForDeps;
    stage2.add_dependency("stage-1".to_string());
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    // Create stage 3 (depends on stage 2)
    let mut stage3 = Stage::new("Stage 3".to_string(), Some("Third stage".to_string()));
    stage3.id = "stage-3".to_string();
    stage3.status = StageStatus::WaitingForDeps;
    stage3.add_dependency("stage-2".to_string());
    save_stage(&stage3, work_dir).expect("Should save stage 3");

    // Complete stage 1
    complete_stage("stage-1", work_dir).expect("Should complete stage 1");
    transition_stage("stage-1", StageStatus::Completed, work_dir).expect("Should verify stage 1");
    trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");

    // Verify stage 2 is Ready, stage 3 still Pending
    let loaded_stage2 = load_stage("stage-2", work_dir).expect("Should load stage 2");
    assert_eq!(loaded_stage2.status, StageStatus::Queued);

    let loaded_stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(loaded_stage3.status, StageStatus::WaitingForDeps);

    // Complete stage 2
    complete_stage("stage-2", work_dir).expect("Should complete stage 2");
    transition_stage("stage-2", StageStatus::Completed, work_dir).expect("Should verify stage 2");
    trigger_dependents("stage-2", work_dir).expect("Should trigger dependents");

    // Verify stage 3 is now Ready
    let loaded_stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(loaded_stage3.status, StageStatus::Queued);

    // Complete stage 3
    complete_stage("stage-3", work_dir).expect("Should complete stage 3");
    transition_stage("stage-3", StageStatus::Completed, work_dir).expect("Should verify stage 3");

    // Verify all stages are Verified
    let stages = list_all_stages(work_dir).expect("Should list all stages");
    assert_eq!(stages.len(), 3);

    for stage in stages {
        assert_eq!(
            stage.status,
            StageStatus::Completed,
            "Stage {} should be Verified",
            stage.id
        );
    }
}

/// Test that deep dependency chains are respected
///
/// Creates: A -> B -> C -> D
/// Verifies:
/// - At each step, only the immediate next stage becomes Ready
/// - No stage skips ahead in the chain
#[test]
fn test_dependency_chain_respected() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create stage 1 (no dependencies)
    let mut stage1 = Stage::new("Stage A".to_string(), Some("First stage".to_string()));
    stage1.id = "stage-a".to_string();
    stage1.status = StageStatus::Queued;
    save_stage(&stage1, work_dir).expect("Should save stage A");

    // Create stage 2 (depends on stage 1)
    let mut stage2 = Stage::new("Stage B".to_string(), Some("Second stage".to_string()));
    stage2.id = "stage-b".to_string();
    stage2.status = StageStatus::WaitingForDeps;
    stage2.add_dependency("stage-a".to_string());
    save_stage(&stage2, work_dir).expect("Should save stage B");

    // Create stage 3 (depends on stage 2)
    let mut stage3 = Stage::new("Stage C".to_string(), Some("Third stage".to_string()));
    stage3.id = "stage-c".to_string();
    stage3.status = StageStatus::WaitingForDeps;
    stage3.add_dependency("stage-b".to_string());
    save_stage(&stage3, work_dir).expect("Should save stage C");

    // Create stage 4 (depends on stage 3)
    let mut stage4 = Stage::new("Stage D".to_string(), Some("Fourth stage".to_string()));
    stage4.id = "stage-d".to_string();
    stage4.status = StageStatus::WaitingForDeps;
    stage4.add_dependency("stage-c".to_string());
    save_stage(&stage4, work_dir).expect("Should save stage D");

    // Complete stage A
    complete_stage("stage-a", work_dir).expect("Should complete stage A");
    transition_stage("stage-a", StageStatus::Completed, work_dir).expect("Should verify stage A");

    let triggered = trigger_dependents("stage-a", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-b");

    // Verify only stage B is Ready, C and D still Pending
    let loaded_stage_b = load_stage("stage-b", work_dir).expect("Should load stage B");
    assert_eq!(loaded_stage_b.status, StageStatus::Queued);

    let loaded_stage_c = load_stage("stage-c", work_dir).expect("Should load stage C");
    assert_eq!(loaded_stage_c.status, StageStatus::WaitingForDeps);

    let loaded_stage_d = load_stage("stage-d", work_dir).expect("Should load stage D");
    assert_eq!(loaded_stage_d.status, StageStatus::WaitingForDeps);

    // Complete stage B
    complete_stage("stage-b", work_dir).expect("Should complete stage B");
    transition_stage("stage-b", StageStatus::Completed, work_dir).expect("Should verify stage B");

    let triggered = trigger_dependents("stage-b", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-c");

    // Verify only stage C is Ready, D still Pending
    let loaded_stage_c = load_stage("stage-c", work_dir).expect("Should load stage C");
    assert_eq!(loaded_stage_c.status, StageStatus::Queued);

    let loaded_stage_d = load_stage("stage-d", work_dir).expect("Should load stage D");
    assert_eq!(loaded_stage_d.status, StageStatus::WaitingForDeps);

    // Complete stage C
    complete_stage("stage-c", work_dir).expect("Should complete stage C");
    transition_stage("stage-c", StageStatus::Completed, work_dir).expect("Should verify stage C");

    let triggered = trigger_dependents("stage-c", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-d");

    // Verify stage D is now Ready
    let loaded_stage_d = load_stage("stage-d", work_dir).expect("Should load stage D");
    assert_eq!(loaded_stage_d.status, StageStatus::Queued);
}

/// Test that stage completion updates timestamps
///
/// Verifies:
/// - completed_at is set when stage is completed
/// - updated_at is set when stage status changes
#[test]
fn test_stage_completion_updates_timestamp() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create stage 1
    let mut stage1 = Stage::new("Stage 1".to_string(), Some("Test stage".to_string()));
    stage1.id = "stage-1".to_string();
    stage1.status = StageStatus::Queued;
    let created_at = stage1.created_at;
    let initial_updated_at = stage1.updated_at;

    save_stage(&stage1, work_dir).expect("Should save stage 1");

    // Wait a tiny bit to ensure timestamps differ
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Complete the stage
    let completed_stage = complete_stage("stage-1", work_dir).expect("Should complete stage 1");

    assert_eq!(completed_stage.status, StageStatus::Completed);
    assert!(
        completed_stage.completed_at.is_some(),
        "completed_at should be set"
    );
    assert!(
        completed_stage.updated_at > initial_updated_at,
        "updated_at should be updated"
    );
    assert_eq!(
        completed_stage.created_at, created_at,
        "created_at should not change"
    );

    // Verify the timestamps persist when reloaded
    let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage 1");
    assert!(reloaded.completed_at.is_some());
    assert_eq!(
        reloaded.completed_at, completed_stage.completed_at,
        "completed_at should persist"
    );
    assert_eq!(
        reloaded.updated_at, completed_stage.updated_at,
        "updated_at should persist"
    );
}
