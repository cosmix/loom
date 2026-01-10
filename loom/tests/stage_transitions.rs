//! Integration tests for stage transitions and dependency triggering

use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::{
    list_all_stages, load_stage, save_stage, transition_stage, trigger_dependents,
};
use tempfile::TempDir;

#[test]
fn test_stage_transition_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

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

    // List all stages
    let stages = list_all_stages(work_dir).expect("Should list stages");
    assert_eq!(stages.len(), 3);

    // Complete and verify stage 1 (following proper state machine)
    let stage1 = transition_stage("stage-1", StageStatus::Executing, work_dir)
        .expect("Should start executing stage 1");
    assert_eq!(stage1.status, StageStatus::Executing);

    let stage1 = transition_stage("stage-1", StageStatus::Completed, work_dir)
        .expect("Should complete stage 1");
    assert_eq!(stage1.status, StageStatus::Completed);

    let stage1 = transition_stage("stage-1", StageStatus::Completed, work_dir)
        .expect("Should verify stage 1");
    assert_eq!(stage1.status, StageStatus::Completed);

    // Trigger dependents - should mark stage 2 as Ready
    let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-2");

    let stage2 = load_stage("stage-2", work_dir).expect("Should load stage 2");
    assert_eq!(stage2.status, StageStatus::Queued);

    // Stage 3 should still be Pending (stage 2 not verified yet)
    let stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(stage3.status, StageStatus::WaitingForDeps);

    // Complete and verify stage 2 (following proper state machine)
    transition_stage("stage-2", StageStatus::Executing, work_dir)
        .expect("Should start executing stage 2");
    transition_stage("stage-2", StageStatus::Completed, work_dir).expect("Should complete stage 2");
    transition_stage("stage-2", StageStatus::Completed, work_dir).expect("Should verify stage 2");

    // Trigger dependents - should mark stage 3 as Ready
    let triggered = trigger_dependents("stage-2", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-3");

    let stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(stage3.status, StageStatus::Queued);
}

#[test]
fn test_multiple_dependencies_all_satisfied() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stage 1
    let mut stage1 = Stage::new("Stage 1".to_string(), None);
    stage1.id = "stage-1".to_string();
    stage1.status = StageStatus::Completed;
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    // Create stage 2
    let mut stage2 = Stage::new("Stage 2".to_string(), None);
    stage2.id = "stage-2".to_string();
    stage2.status = StageStatus::WaitingForDeps;
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    // Create stage 3 that depends on both stage 1 and stage 2
    let mut stage3 = Stage::new("Stage 3".to_string(), None);
    stage3.id = "stage-3".to_string();
    stage3.status = StageStatus::WaitingForDeps;
    stage3.add_dependency("stage-1".to_string());
    stage3.add_dependency("stage-2".to_string());
    save_stage(&stage3, work_dir).expect("Should save stage 3");

    // Verify stage 1 - should not trigger stage 3 yet
    let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 0);

    let stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(stage3.status, StageStatus::WaitingForDeps);

    // Complete and verify stage 2 (following proper state machine)
    transition_stage("stage-2", StageStatus::Queued, work_dir).expect("Should mark stage 2 ready");
    transition_stage("stage-2", StageStatus::Executing, work_dir)
        .expect("Should start executing stage 2");
    transition_stage("stage-2", StageStatus::Completed, work_dir).expect("Should complete stage 2");
    transition_stage("stage-2", StageStatus::Completed, work_dir).expect("Should verify stage 2");

    let triggered = trigger_dependents("stage-2", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-3");

    let stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(stage3.status, StageStatus::Queued);
}

#[test]
fn test_parallel_stages() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stage 1 (foundation)
    let mut stage1 = Stage::new("Stage 1".to_string(), None);
    stage1.id = "stage-1".to_string();
    stage1.status = StageStatus::Completed;
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    // Create stages 2 and 3 (both depend on stage 1, can run in parallel)
    let mut stage2 = Stage::new("Stage 2".to_string(), None);
    stage2.id = "stage-2".to_string();
    stage2.status = StageStatus::WaitingForDeps;
    stage2.add_dependency("stage-1".to_string());
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    let mut stage3 = Stage::new("Stage 3".to_string(), None);
    stage3.id = "stage-3".to_string();
    stage3.status = StageStatus::WaitingForDeps;
    stage3.add_dependency("stage-1".to_string());
    save_stage(&stage3, work_dir).expect("Should save stage 3");

    // Trigger dependents - both stage 2 and stage 3 should become Ready
    let mut triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");
    triggered.sort(); // Sort for deterministic comparison
    assert_eq!(triggered.len(), 2);
    assert!(triggered.contains(&"stage-2".to_string()));
    assert!(triggered.contains(&"stage-3".to_string()));

    let stage2 = load_stage("stage-2", work_dir).expect("Should load stage 2");
    assert_eq!(stage2.status, StageStatus::Queued);

    let stage3 = load_stage("stage-3", work_dir).expect("Should load stage 3");
    assert_eq!(stage3.status, StageStatus::Queued);
}
