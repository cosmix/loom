//! Integration tests for stage completion flow
//!
//! These tests verify the end-to-end completion flow including:
//! - Dependency satisfaction requiring merged=true
//! - Acceptance failure leading to CompletedWithFailures
//! - Successful completion triggering dependents
//!
//! Test cases as specified by the signal:
//! 1. test_completion_requires_merged_dependencies
//! 2. test_acceptance_failure_creates_correct_status
//! 3. test_successful_completion_flow

use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::{
    are_all_dependencies_satisfied, load_stage, save_stage, trigger_dependents,
};
use tempfile::TempDir;

/// Helper to create a test stage with a specific ID and status
fn create_test_stage(id: &str, name: &str, status: StageStatus) -> Stage {
    let mut stage = Stage::new(name.to_string(), Some(format!("Test stage {name}")));
    stage.id = id.to_string();
    stage.status = status;
    stage
}

/// Test: A stage in Completed status but NOT merged should not satisfy dependencies.
///
/// This tests the core invariant: dependents should only run when their dependencies
/// are both Completed AND merged to the merge point.
#[test]
fn test_completion_requires_merged_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create parent stage: Completed but NOT merged
    let mut parent = create_test_stage("parent-stage", "Parent Stage", StageStatus::Executing);
    parent.try_complete(Some("Work done".to_string())).unwrap();
    // Explicitly set merged = false (this is the default but being explicit)
    parent.merged = false;
    save_stage(&parent, work_dir).expect("Should save parent stage");

    // Create child stage depending on parent
    let mut child = create_test_stage("child-stage", "Child Stage", StageStatus::WaitingForDeps);
    child.add_dependency("parent-stage".to_string());
    save_stage(&child, work_dir).expect("Should save child stage");

    // Assert: Child dependencies NOT satisfied because parent.merged = false
    let satisfied =
        are_all_dependencies_satisfied(&child, work_dir).expect("Should check dependencies");
    assert!(
        !satisfied,
        "Child dependencies should NOT be satisfied when parent is Completed but merged=false"
    );

    // Attempt to trigger dependents - child should NOT transition to Queued
    let triggered =
        trigger_dependents("parent-stage", work_dir).expect("Should trigger dependents");
    assert!(
        triggered.is_empty(),
        "No stages should be triggered when parent.merged = false"
    );

    // Verify child is still WaitingForDeps
    let child = load_stage("child-stage", work_dir).expect("Should load child");
    assert_eq!(
        child.status,
        StageStatus::WaitingForDeps,
        "Child should remain in WaitingForDeps"
    );

    // Now set parent.merged = true and try again
    let mut parent = load_stage("parent-stage", work_dir).expect("Should load parent");
    parent.merged = true;
    save_stage(&parent, work_dir).expect("Should save parent with merged=true");

    // Assert: Child dependencies NOW satisfied
    let child = load_stage("child-stage", work_dir).expect("Should reload child");
    let satisfied =
        are_all_dependencies_satisfied(&child, work_dir).expect("Should check dependencies");
    assert!(
        satisfied,
        "Child dependencies should be satisfied when parent is Completed AND merged=true"
    );

    // Trigger dependents again - child SHOULD transition to Queued
    let triggered =
        trigger_dependents("parent-stage", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1, "One stage should be triggered");
    assert_eq!(
        triggered[0], "child-stage",
        "Child stage should be triggered"
    );

    // Verify child is now Queued
    let child = load_stage("child-stage", work_dir).expect("Should reload child");
    assert_eq!(
        child.status,
        StageStatus::Queued,
        "Child should now be Queued"
    );
}

/// Test: Stage completing with acceptance failure should transition to CompletedWithFailures.
///
/// The CompletedWithFailures status indicates work was done but verification failed.
/// Dependent stages should NOT be triggered.
#[test]
fn test_acceptance_failure_creates_correct_status() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create a stage that's executing
    let mut stage = create_test_stage("test-stage", "Test Stage", StageStatus::Executing);
    stage.add_acceptance_criterion("false".to_string()); // Criterion that will fail
    save_stage(&stage, work_dir).expect("Should save test stage");

    // Create dependent stage
    let mut dependent = create_test_stage(
        "dependent-stage",
        "Dependent Stage",
        StageStatus::WaitingForDeps,
    );
    dependent.add_dependency("test-stage".to_string());
    save_stage(&dependent, work_dir).expect("Should save dependent stage");

    // Simulate acceptance failure by calling try_complete_with_failures
    let mut stage = load_stage("test-stage", work_dir).expect("Should load test stage");
    stage
        .try_complete_with_failures()
        .expect("Should transition to CompletedWithFailures");
    save_stage(&stage, work_dir).expect("Should save stage with CompletedWithFailures");

    // Assert: Stage status is CompletedWithFailures
    let stage = load_stage("test-stage", work_dir).expect("Should reload test stage");
    assert_eq!(
        stage.status,
        StageStatus::CompletedWithFailures,
        "Stage should be in CompletedWithFailures status"
    );

    // Assert: Dependents NOT triggered (CompletedWithFailures doesn't satisfy deps)
    let triggered = trigger_dependents("test-stage", work_dir).expect("Should trigger dependents");
    assert!(
        triggered.is_empty(),
        "No stages should be triggered when parent is CompletedWithFailures"
    );

    // Assert: Dependent still in WaitingForDeps
    let dependent = load_stage("dependent-stage", work_dir).expect("Should reload dependent");
    assert_eq!(
        dependent.status,
        StageStatus::WaitingForDeps,
        "Dependent should remain in WaitingForDeps"
    );

    // CompletedWithFailures should be retryable -> Executing
    let mut stage = load_stage("test-stage", work_dir).expect("Should reload test stage");
    let can_retry = stage.status.can_transition_to(&StageStatus::Executing);
    assert!(
        can_retry,
        "CompletedWithFailures should be retryable to Executing"
    );

    stage
        .try_mark_executing()
        .expect("Should transition to Executing for retry");
    save_stage(&stage, work_dir).expect("Should save stage after retry");
    assert_eq!(
        stage.status,
        StageStatus::Executing,
        "Stage should be Executing after retry"
    );
}

/// Test: Successful completion sets stage.merged = true and triggers dependents.
///
/// This is the happy path where acceptance passes and merge succeeds.
#[test]
fn test_successful_completion_flow() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create parent stage in Executing status
    let parent = create_test_stage("parent-stage", "Parent Stage", StageStatus::Executing);
    save_stage(&parent, work_dir).expect("Should save parent stage");

    // Create child stage depending on parent
    let mut child = create_test_stage("child-stage", "Child Stage", StageStatus::WaitingForDeps);
    child.add_dependency("parent-stage".to_string());
    save_stage(&child, work_dir).expect("Should save child stage");

    // Simulate successful completion (acceptance passed, merge succeeded)
    let mut parent = load_stage("parent-stage", work_dir).expect("Should load parent");
    parent
        .try_complete(Some("Success!".to_string()))
        .expect("Should complete");
    parent.merged = true; // Merge succeeded
    save_stage(&parent, work_dir).expect("Should save completed parent");

    // Assert: Stage status is Completed
    let parent = load_stage("parent-stage", work_dir).expect("Should reload parent");
    assert_eq!(
        parent.status,
        StageStatus::Completed,
        "Stage should be Completed"
    );

    // Assert: Stage merged is true
    assert!(
        parent.merged,
        "Stage should have merged=true after successful completion"
    );

    // Assert: completed_at is set
    assert!(
        parent.completed_at.is_some(),
        "Stage should have completed_at timestamp"
    );

    // Trigger dependents - child should transition
    let triggered =
        trigger_dependents("parent-stage", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1, "Child stage should be triggered");
    assert_eq!(triggered[0], "child-stage");

    // Assert: Child is now Queued
    let child = load_stage("child-stage", work_dir).expect("Should reload child");
    assert_eq!(
        child.status,
        StageStatus::Queued,
        "Child should be Queued after parent completion"
    );
}
