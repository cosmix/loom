//! Tests for trigger_dependents functionality
//!
//! These tests set dep stages to `StageType::Knowledge` to bypass the git
//! ancestry check added in `PLAN-fix-phantom-merge.md` (Fix 9). Knowledge
//! stages have no branch by design, so dependencies satisfied via the
//! metadata-only path are exercised here. Real-git regression tests that
//! exercise the ancestry check live in a separate test file (added as part
//! of the plan's integration-verify stage).

use tempfile::TempDir;

use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::create_test_stage;

#[test]
fn test_trigger_dependents_single_dependency() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let mut stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage1.stage_type = StageType::Knowledge; // Bypass git ancestry check
    stage1.merged = true; // Dependency must be merged to satisfy
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
    stage2.add_dependency("stage-1".to_string());
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    let triggered = trigger_dependents("stage-1", work_dir, work_dir, "main")
        .expect("Should trigger dependents");

    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-2");

    let reloaded = load_stage("stage-2", work_dir).expect("Should reload stage 2");
    assert_eq!(reloaded.status, StageStatus::Queued);
}

#[test]
fn test_trigger_dependents_multiple_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let mut stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage1.stage_type = StageType::Knowledge; // Bypass git ancestry check
    stage1.merged = true; // Dependency must be merged to satisfy
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    let mut stage3 = create_test_stage("stage-3", "Stage 3", StageStatus::WaitingForDeps);
    stage3.add_dependency("stage-1".to_string());
    stage3.add_dependency("stage-2".to_string());
    save_stage(&stage3, work_dir).expect("Should save stage 3");

    let triggered = trigger_dependents("stage-1", work_dir, work_dir, "main")
        .expect("Should trigger dependents");
    assert_eq!(triggered.len(), 0);

    let mut stage2_completed = create_test_stage("stage-2", "Stage 2", StageStatus::Completed);
    stage2_completed.stage_type = StageType::Knowledge; // Bypass git ancestry check
    stage2_completed.merged = true; // Dependency must be merged to satisfy
    save_stage(&stage2_completed, work_dir).expect("Should save stage 2");

    let triggered = trigger_dependents("stage-2", work_dir, work_dir, "main")
        .expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0], "stage-3");

    let reloaded = load_stage("stage-3", work_dir).expect("Should reload stage 3");
    assert_eq!(reloaded.status, StageStatus::Queued);
}

#[test]
fn test_trigger_dependents_no_dependents() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let triggered = trigger_dependents("stage-1", work_dir, work_dir, "main")
        .expect("Should trigger dependents");

    assert_eq!(triggered.len(), 0);
}

#[test]
fn test_trigger_dependents_already_ready() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::Queued);
    stage2.add_dependency("stage-1".to_string());
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    let triggered = trigger_dependents("stage-1", work_dir, work_dir, "main")
        .expect("Should trigger dependents");

    assert_eq!(triggered.len(), 0);
}
