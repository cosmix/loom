//! Tests for are_all_dependencies_satisfied functionality
//!
//! These tests set dep stages to `StageType::Knowledge` to bypass the git
//! ancestry check added in `PLAN-fix-phantom-merge.md` (Fix 9). Knowledge
//! stages have no branch by design, so dependencies satisfied via the
//! metadata-only path are exercised here. Real-git regression tests that
//! exercise the ancestry check live in a separate test file (added as part
//! of the plan's integration-verify stage).

use tempfile::TempDir;

use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::{are_all_dependencies_satisfied, save_stage};

use super::create_test_stage;

#[test]
fn are_all_dependencies_satisfied_empty() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Stage 1", StageStatus::WaitingForDeps);

    let satisfied = are_all_dependencies_satisfied(&stage, work_dir, work_dir, "main")
        .expect("Should check dependencies");

    assert!(satisfied);
}

#[test]
fn are_all_dependencies_satisfied_true() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let mut stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage1.stage_type = StageType::Knowledge; // Bypass git ancestry check
    stage1.merged = true; // Must be merged to satisfy dependency
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
    stage2.add_dependency("stage-1".to_string());

    let satisfied = are_all_dependencies_satisfied(&stage2, work_dir, work_dir, "main")
        .expect("Should check dependencies");

    assert!(satisfied);
}

#[test]
fn are_all_dependencies_satisfied_false() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let mut stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::WaitingForDeps);
    stage1.stage_type = StageType::Knowledge; // Bypass git ancestry check
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
    stage2.add_dependency("stage-1".to_string());

    let satisfied = are_all_dependencies_satisfied(&stage2, work_dir, work_dir, "main")
        .expect("Should check dependencies");

    assert!(!satisfied);
}

#[test]
fn are_all_dependencies_satisfied_requires_merged_flag() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create dependency stage: Completed but NOT merged.
    // Set stage_type to Knowledge so we bypass the new git ancestry check
    // and isolate testing of the `merged` flag requirement.
    let mut dep_stage = create_test_stage("dep-stage", "Dependency", StageStatus::Completed);
    dep_stage.stage_type = StageType::Knowledge;
    dep_stage.merged = false; // Explicitly not merged
    save_stage(&dep_stage, work_dir).expect("Should save dep stage");

    let mut dependent_stage =
        create_test_stage("dependent-stage", "Dependent", StageStatus::WaitingForDeps);
    dependent_stage.add_dependency("dep-stage".to_string());

    // Should return false: Completed but not merged
    let satisfied = are_all_dependencies_satisfied(&dependent_stage, work_dir, work_dir, "main")
        .expect("Should check dependencies");
    assert!(
        !satisfied,
        "Dependency should NOT be satisfied when Completed but merged=false"
    );

    // Now set merged = true
    dep_stage.merged = true;
    save_stage(&dep_stage, work_dir).expect("Should save dep stage with merged=true");

    // Should return true: Completed AND merged (knowledge stages skip ancestry check)
    let satisfied = are_all_dependencies_satisfied(&dependent_stage, work_dir, work_dir, "main")
        .expect("Should check dependencies");
    assert!(
        satisfied,
        "Dependency should be satisfied when Completed AND merged=true (knowledge stage)"
    );
}
