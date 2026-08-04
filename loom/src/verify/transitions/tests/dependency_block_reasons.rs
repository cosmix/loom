//! Tests for `describe_dependency_block` — the diagnostic behind the
//! "queued for X, reason Y" line in `loom status`.
//!
//! The classification these tests pin down is the useful part: whether the
//! orchestrator will clear the block on its own (`self_resolving`) or the plan
//! is parked until a human acts. Getting that wrong turns a real dead end into
//! a "probably fine, give it a minute", which is how a stalled plan goes
//! unnoticed for hours.
//!
//! Dep stages are `StageType::Knowledge` where the test is not about git, to
//! bypass the ancestry check (see `dependency_satisfaction.rs`).

use tempfile::TempDir;

use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::{describe_dependency_block, save_stage};

use super::create_test_stage;

/// Save a dependency in `status`, then ask why `dependent` cannot run.
fn block_for_dep_status(status: StageStatus, merged: bool) -> Option<(String, String, bool)> {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let mut dep = create_test_stage("dep", "Dep", status);
    dep.stage_type = StageType::Knowledge;
    dep.merged = merged;
    save_stage(&dep, work_dir).expect("Should save dep");

    let mut dependent = create_test_stage("dependent", "Dependent", StageStatus::Queued);
    dependent.add_dependency("dep".to_string());

    describe_dependency_block(&dependent, work_dir, work_dir, "main")
        .expect("Should describe block")
        .map(|b| (b.dependency, b.detail, b.self_resolving))
}

#[test]
fn satisfied_dependency_reports_no_block() {
    assert!(block_for_dep_status(StageStatus::Completed, true).is_none());
}

#[test]
fn stage_with_no_dependencies_reports_no_block() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("solo", "Solo", StageStatus::Queued);

    assert!(
        describe_dependency_block(&stage, work_dir, work_dir, "main")
            .expect("Should describe block")
            .is_none()
    );
}

#[test]
fn completed_but_unmerged_dependency_is_self_resolving() {
    // Auto-merge will clear this without help — the common transient case.
    let (dep, detail, self_resolving) =
        block_for_dep_status(StageStatus::Completed, false).expect("should block");

    assert_eq!(dep, "dep");
    assert!(detail.contains("not merged"), "got: {detail}");
    assert!(self_resolving);
}

#[test]
fn running_dependency_is_self_resolving() {
    let (_, detail, self_resolving) =
        block_for_dep_status(StageStatus::Executing, false).expect("should block");

    assert!(detail.contains("Executing"), "got: {detail}");
    assert!(self_resolving);
}

#[test]
fn blocked_dependency_needs_intervention() {
    let (_, detail, self_resolving) =
        block_for_dep_status(StageStatus::Blocked, false).expect("should block");

    assert!(detail.contains("Blocked"), "got: {detail}");
    assert!(
        !self_resolving,
        "a Blocked dependency does not clear itself; reporting it as transient \
         hides a dead plan"
    );
}

#[test]
fn skipped_dependency_is_a_permanent_dead_end() {
    // A Skipped stage never becomes Completed+merged, so nothing downstream
    // can ever become ready. This is the state most likely to be mistaken for
    // "still working on it".
    let (_, detail, self_resolving) =
        block_for_dep_status(StageStatus::Skipped, false).expect("should block");

    assert!(detail.contains("Skipped"), "got: {detail}");
    assert!(detail.contains("never"), "must say it will never resolve");
    assert!(!self_resolving);
}

#[test]
fn merge_conflict_dependency_needs_intervention() {
    let (_, detail, self_resolving) =
        block_for_dep_status(StageStatus::MergeConflict, false).expect("should block");

    assert!(detail.contains("merge resolution"), "got: {detail}");
    assert!(!self_resolving);
}

#[test]
fn adjudication_dependency_reports_waiting_on_a_verdict() {
    let (_, detail, self_resolving) =
        block_for_dep_status(StageStatus::NeedsAdjudication, false).expect("should block");

    assert!(detail.contains("verdict"), "got: {detail}");
    assert!(!self_resolving);
}

#[test]
fn human_review_dependency_reports_waiting_on_a_verdict() {
    let (_, detail, self_resolving) =
        block_for_dep_status(StageStatus::NeedsHumanReview, false).expect("should block");

    assert!(detail.contains("verdict"), "got: {detail}");
    assert!(!self_resolving);
}

#[test]
fn completed_with_failures_dependency_needs_intervention() {
    let (_, detail, self_resolving) =
        block_for_dep_status(StageStatus::CompletedWithFailures, false).expect("should block");

    assert!(detail.contains("CompletedWithFailures"), "got: {detail}");
    assert!(!self_resolving);
}

#[test]
fn unreadable_dependency_file_is_reported_not_swallowed() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Dependency named in the plan but never written to disk.
    let mut dependent = create_test_stage("dependent", "Dependent", StageStatus::Queued);
    dependent.add_dependency("ghost".to_string());

    let block = describe_dependency_block(&dependent, work_dir, work_dir, "main")
        .expect("Should describe block")
        .expect("a missing dependency file must be reported");

    assert_eq!(block.dependency, "ghost");
    assert!(!block.self_resolving);
}

#[test]
fn first_blocking_dependency_is_reported_when_several_are_unsatisfied() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    for id in ["dep-a", "dep-b"] {
        let mut dep = create_test_stage(id, id, StageStatus::Blocked);
        dep.stage_type = StageType::Knowledge;
        save_stage(&dep, work_dir).expect("Should save dep");
    }

    let mut dependent = create_test_stage("dependent", "Dependent", StageStatus::Queued);
    dependent.add_dependency("dep-a".to_string());
    dependent.add_dependency("dep-b".to_string());

    let block = describe_dependency_block(&dependent, work_dir, work_dir, "main")
        .expect("Should describe block")
        .expect("should block");

    // Dependency order, not filesystem order — the report must be stable.
    assert_eq!(block.dependency, "dep-a");
}

#[test]
fn merged_dependency_without_a_commit_is_not_treated_as_satisfied() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Non-knowledge stage claiming merged=true with nothing to verify against.
    let mut dep = create_test_stage("dep", "Dep", StageStatus::Completed);
    dep.merged = true;
    dep.completed_commit = None;
    save_stage(&dep, work_dir).expect("Should save dep");

    let mut dependent = create_test_stage("dependent", "Dependent", StageStatus::Queued);
    dependent.add_dependency("dep".to_string());

    let block = describe_dependency_block(&dependent, work_dir, work_dir, "main")
        .expect("Should describe block")
        .expect("an unverifiable merge must block");

    assert!(
        block.detail.contains("completed_commit"),
        "{}",
        block.detail
    );
    assert!(!block.self_resolving);
}
