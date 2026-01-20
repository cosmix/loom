//! Tests for stage persistence (load, save, list)

use tempfile::TempDir;

use crate::models::stage::StageStatus;
use crate::verify::transitions::{list_all_stages, load_stage, save_stage};

use super::create_test_stage;

#[test]
fn test_load_and_save_stage() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);

    save_stage(&stage, work_dir).expect("Should save stage");

    let loaded = load_stage("stage-1", work_dir).expect("Should load stage");

    assert_eq!(loaded.id, stage.id);
    assert_eq!(loaded.name, stage.name);
    assert_eq!(loaded.status, stage.status);
    assert_eq!(loaded.description, stage.description);
}

#[test]
fn test_load_nonexistent_stage() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let result = load_stage("nonexistent", work_dir);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_list_all_stages() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::WaitingForDeps);
    let stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::Queued);
    let stage3 = create_test_stage("stage-3", "Stage 3", StageStatus::Completed);

    save_stage(&stage1, work_dir).expect("Should save stage 1");
    save_stage(&stage2, work_dir).expect("Should save stage 2");
    save_stage(&stage3, work_dir).expect("Should save stage 3");

    let stages = list_all_stages(work_dir).expect("Should list stages");

    assert_eq!(stages.len(), 3);

    let ids: Vec<String> = stages.iter().map(|s| s.id.clone()).collect();
    assert!(ids.contains(&"stage-1".to_string()));
    assert!(ids.contains(&"stage-2".to_string()));
    assert!(ids.contains(&"stage-3".to_string()));
}

#[test]
fn test_list_all_stages_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stages = list_all_stages(work_dir).expect("Should handle empty directory");
    assert_eq!(stages.len(), 0);
}
