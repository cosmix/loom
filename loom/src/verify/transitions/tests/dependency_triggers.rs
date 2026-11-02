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
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents, update_stage};

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

    update_stage("stage-2", work_dir, |stage| {
        stage.status = StageStatus::Completed;
        stage.stage_type = StageType::Knowledge; // Bypass git ancestry check
        stage.merged = true; // Dependency must be merged to satisfy
        Ok(())
    })
    .expect("Should save stage 2");

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

/// One dependent whose ancestry check errors must not cost its siblings their
/// transition: the caller has already recorded the merge and still has to
/// clean up. The failing dependent stays waiting for the next poll.
#[test]
fn an_ancestry_error_on_one_dependent_still_queues_the_others() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let mut done = create_test_stage("done", "Done", StageStatus::Completed);
    done.stage_type = StageType::Knowledge;
    done.merged = true;
    save_stage(&done, work_dir).unwrap();

    // Not a repository, so `merge-base --is-ancestor` fails instead of answering.
    let mut unproven = create_test_stage("unproven", "Unproven", StageStatus::Completed);
    unproven.merged = true;
    unproven.completed_commit = Some("0123456789abcdef".to_string());
    save_stage(&unproven, work_dir).unwrap();

    for id in ["a-errors", "z-errors"] {
        let mut failing = create_test_stage(id, id, StageStatus::WaitingForDeps);
        failing.add_dependency("done".to_string());
        failing.add_dependency("unproven".to_string());
        save_stage(&failing, work_dir).unwrap();
    }
    let mut ready = create_test_stage("m-ready", "Ready", StageStatus::WaitingForDeps);
    ready.add_dependency("done".to_string());
    save_stage(&ready, work_dir).unwrap();

    let triggered = trigger_dependents("done", work_dir, work_dir, "main").unwrap();

    assert_eq!(triggered, ["m-ready"]);
    assert_eq!(
        load_stage("m-ready", work_dir).unwrap().status,
        StageStatus::Queued
    );
    for id in ["a-errors", "z-errors"] {
        assert_eq!(
            load_stage(id, work_dir).unwrap().status,
            StageStatus::WaitingForDeps
        );
    }
}
