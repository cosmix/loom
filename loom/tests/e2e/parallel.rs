//! E2E tests for parallel stage execution scenarios
//!
//! Tests parallel stage triggering, parallel group isolation, and complex
//! dependency patterns like diamond and fan-out/fan-in.

use super::helpers::complete_stage;
use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::{load_stage, save_stage, transition_stage, trigger_dependents};
use tempfile::TempDir;

#[test]
fn test_parallel_stages_triggered_together() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create foundation stage
    let mut foundation = Stage::new("Foundation".to_string(), None);
    foundation.id = "foundation".to_string();
    foundation.status = StageStatus::Completed;
    save_stage(&foundation, work_dir).unwrap();

    // Create parallel stages depending on foundation
    for (name, id) in [
        ("Stage A", "stage-a"),
        ("Stage B", "stage-b"),
        ("Stage C", "stage-c"),
    ] {
        let mut stage = Stage::new(name.to_string(), None);
        stage.id = id.to_string();
        stage.status = StageStatus::WaitingForDeps;
        stage.add_dependency("foundation".to_string());
        stage.parallel_group = Some("parallel".to_string());
        save_stage(&stage, work_dir).unwrap();
    }

    // Verify foundation stage is Ready, A/B/C are Pending
    let foundation = load_stage("foundation", work_dir).unwrap();
    assert_eq!(foundation.status, StageStatus::Completed);

    for id in ["stage-a", "stage-b", "stage-c"] {
        let stage = load_stage(id, work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::WaitingForDeps);
    }

    // Trigger dependents
    let mut triggered = trigger_dependents("foundation", work_dir).unwrap();
    triggered.sort();
    assert_eq!(triggered.len(), 3);
    assert_eq!(
        triggered,
        vec![
            "stage-a".to_string(),
            "stage-b".to_string(),
            "stage-c".to_string()
        ]
    );

    // Verify all became Ready simultaneously
    for id in ["stage-a", "stage-b", "stage-c"] {
        let stage = load_stage(id, work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::Queued);
        assert_eq!(stage.parallel_group, Some("parallel".to_string()));
    }
}

#[test]
fn test_parallel_group_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create foundation stage
    let mut foundation = Stage::new("Foundation".to_string(), None);
    foundation.id = "foundation".to_string();
    foundation.status = StageStatus::Completed;
    save_stage(&foundation, work_dir).unwrap();

    // Create frontend parallel group (A, B)
    for (name, id) in [("Frontend A", "frontend-a"), ("Frontend B", "frontend-b")] {
        let mut stage = Stage::new(name.to_string(), None);
        stage.id = id.to_string();
        stage.status = StageStatus::WaitingForDeps;
        stage.add_dependency("foundation".to_string());
        stage.parallel_group = Some("frontend".to_string());
        save_stage(&stage, work_dir).unwrap();
    }

    // Create backend parallel group (C, D)
    for (name, id) in [("Backend C", "backend-c"), ("Backend D", "backend-d")] {
        let mut stage = Stage::new(name.to_string(), None);
        stage.id = id.to_string();
        stage.status = StageStatus::WaitingForDeps;
        stage.add_dependency("foundation".to_string());
        stage.parallel_group = Some("backend".to_string());
        save_stage(&stage, work_dir).unwrap();
    }

    // Complete foundation
    let mut triggered = trigger_dependents("foundation", work_dir).unwrap();
    triggered.sort();
    assert_eq!(triggered.len(), 4);

    // Verify all 4 stages become Ready
    for id in ["frontend-a", "frontend-b", "backend-c", "backend-d"] {
        let stage = load_stage(id, work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::Queued);
    }

    // Verify stages have correct parallel_group assignments
    let frontend_a = load_stage("frontend-a", work_dir).unwrap();
    assert_eq!(frontend_a.parallel_group, Some("frontend".to_string()));

    let frontend_b = load_stage("frontend-b", work_dir).unwrap();
    assert_eq!(frontend_b.parallel_group, Some("frontend".to_string()));

    let backend_c = load_stage("backend-c", work_dir).unwrap();
    assert_eq!(backend_c.parallel_group, Some("backend".to_string()));

    let backend_d = load_stage("backend-d", work_dir).unwrap();
    assert_eq!(backend_d.parallel_group, Some("backend".to_string()));
}

#[test]
fn test_diamond_dependency_pattern() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create diamond: A -> B, A -> C, B -> D, C -> D
    // A (top)
    let mut stage_a = Stage::new("Stage A".to_string(), None);
    stage_a.id = "stage-a".to_string();
    stage_a.status = StageStatus::Completed;
    save_stage(&stage_a, work_dir).unwrap();

    // B (depends on A)
    let mut stage_b = Stage::new("Stage B".to_string(), None);
    stage_b.id = "stage-b".to_string();
    stage_b.status = StageStatus::WaitingForDeps;
    stage_b.add_dependency("stage-a".to_string());
    save_stage(&stage_b, work_dir).unwrap();

    // C (depends on A)
    let mut stage_c = Stage::new("Stage C".to_string(), None);
    stage_c.id = "stage-c".to_string();
    stage_c.status = StageStatus::WaitingForDeps;
    stage_c.add_dependency("stage-a".to_string());
    save_stage(&stage_c, work_dir).unwrap();

    // D (depends on B and C)
    let mut stage_d = Stage::new("Stage D".to_string(), None);
    stage_d.id = "stage-d".to_string();
    stage_d.status = StageStatus::WaitingForDeps;
    stage_d.add_dependency("stage-b".to_string());
    stage_d.add_dependency("stage-c".to_string());
    save_stage(&stage_d, work_dir).unwrap();

    // Complete A
    let mut triggered = trigger_dependents("stage-a", work_dir).unwrap();
    triggered.sort();
    assert_eq!(triggered.len(), 2);
    assert!(triggered.contains(&"stage-b".to_string()));
    assert!(triggered.contains(&"stage-c".to_string()));

    // Verify B and C become Ready
    let stage_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(stage_b.status, StageStatus::Queued);

    let stage_c = load_stage("stage-c", work_dir).unwrap();
    assert_eq!(stage_c.status, StageStatus::Queued);

    // D should still be Pending
    let stage_d = load_stage("stage-d", work_dir).unwrap();
    assert_eq!(stage_d.status, StageStatus::WaitingForDeps);

    // Complete B only
    complete_stage("stage-b", work_dir).unwrap();
    transition_stage("stage-b", StageStatus::Completed, work_dir).unwrap();
    let triggered = trigger_dependents("stage-b", work_dir).unwrap();
    assert_eq!(triggered.len(), 0);

    // Verify D is still Pending (C not done)
    let stage_d = load_stage("stage-d", work_dir).unwrap();
    assert_eq!(stage_d.status, StageStatus::WaitingForDeps);

    // Complete C
    complete_stage("stage-c", work_dir).unwrap();
    transition_stage("stage-c", StageStatus::Completed, work_dir).unwrap();
    let triggered = trigger_dependents("stage-c", work_dir).unwrap();
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-d");

    // Verify D becomes Ready
    let stage_d = load_stage("stage-d", work_dir).unwrap();
    assert_eq!(stage_d.status, StageStatus::Queued);
}

#[test]
fn test_fan_out_fan_in() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create: A -> {B, C, D} -> E
    // A
    let mut stage_a = Stage::new("Stage A".to_string(), None);
    stage_a.id = "stage-a".to_string();
    stage_a.status = StageStatus::Completed;
    save_stage(&stage_a, work_dir).unwrap();

    // B, C, D all depend on A
    for (name, id) in [
        ("Stage B", "stage-b"),
        ("Stage C", "stage-c"),
        ("Stage D", "stage-d"),
    ] {
        let mut stage = Stage::new(name.to_string(), None);
        stage.id = id.to_string();
        stage.status = StageStatus::WaitingForDeps;
        stage.add_dependency("stage-a".to_string());
        save_stage(&stage, work_dir).unwrap();
    }

    // E depends on B, C, and D
    let mut stage_e = Stage::new("Stage E".to_string(), None);
    stage_e.id = "stage-e".to_string();
    stage_e.status = StageStatus::WaitingForDeps;
    stage_e.add_dependency("stage-b".to_string());
    stage_e.add_dependency("stage-c".to_string());
    stage_e.add_dependency("stage-d".to_string());
    save_stage(&stage_e, work_dir).unwrap();

    // Complete A, verify B/C/D Ready
    let mut triggered = trigger_dependents("stage-a", work_dir).unwrap();
    triggered.sort();
    assert_eq!(triggered.len(), 3);
    assert_eq!(
        triggered,
        vec![
            "stage-b".to_string(),
            "stage-c".to_string(),
            "stage-d".to_string()
        ]
    );

    for id in ["stage-b", "stage-c", "stage-d"] {
        let stage = load_stage(id, work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::Queued);
    }

    // E should still be Pending
    let stage_e = load_stage("stage-e", work_dir).unwrap();
    assert_eq!(stage_e.status, StageStatus::WaitingForDeps);

    // Complete B and C
    for id in ["stage-b", "stage-c"] {
        complete_stage(id, work_dir).unwrap();
        transition_stage(id, StageStatus::Completed, work_dir).unwrap();
        trigger_dependents(id, work_dir).unwrap();
    }

    // Verify E still Pending (D not done)
    let stage_e = load_stage("stage-e", work_dir).unwrap();
    assert_eq!(stage_e.status, StageStatus::WaitingForDeps);

    // Complete D
    complete_stage("stage-d", work_dir).unwrap();
    transition_stage("stage-d", StageStatus::Completed, work_dir).unwrap();
    let triggered = trigger_dependents("stage-d", work_dir).unwrap();
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-e");

    // Verify E becomes Ready
    let stage_e = load_stage("stage-e", work_dir).unwrap();
    assert_eq!(stage_e.status, StageStatus::Queued);
}

#[test]
fn test_parallel_group_assignment() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Create stages with parallel_group set
    let mut stage_a = Stage::new("Stage A".to_string(), None);
    stage_a.id = "stage-a".to_string();
    stage_a.parallel_group = Some("group-1".to_string());
    save_stage(&stage_a, work_dir).unwrap();

    let mut stage_b = Stage::new("Stage B".to_string(), None);
    stage_b.id = "stage-b".to_string();
    stage_b.parallel_group = Some("group-2".to_string());
    save_stage(&stage_b, work_dir).unwrap();

    let mut stage_c = Stage::new("Stage C".to_string(), None);
    stage_c.id = "stage-c".to_string();
    stage_c.parallel_group = None;
    save_stage(&stage_c, work_dir).unwrap();

    // Verify parallel_group field is preserved through save/load
    let loaded_a = load_stage("stage-a", work_dir).unwrap();
    assert_eq!(loaded_a.parallel_group, Some("group-1".to_string()));

    let loaded_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(loaded_b.parallel_group, Some("group-2".to_string()));

    let loaded_c = load_stage("stage-c", work_dir).unwrap();
    assert_eq!(loaded_c.parallel_group, None);

    // Test updating parallel_group
    let mut stage_a = loaded_a;
    stage_a.set_parallel_group(Some("new-group".to_string()));
    save_stage(&stage_a, work_dir).unwrap();

    let reloaded_a = load_stage("stage-a", work_dir).unwrap();
    assert_eq!(reloaded_a.parallel_group, Some("new-group".to_string()));

    // Test clearing parallel_group
    let mut stage_b = loaded_b;
    stage_b.set_parallel_group(None);
    save_stage(&stage_b, work_dir).unwrap();

    let reloaded_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(reloaded_b.parallel_group, None);
}
