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

/// Test: Multiple dependencies all need to be Completed AND merged.
#[test]
fn test_multiple_dependencies_all_must_be_merged() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create two parent stages - one completed+merged, one completed but not merged
    let mut parent1 = create_test_stage("parent-1", "Parent 1", StageStatus::Executing);
    parent1.try_complete(None).unwrap();
    parent1.merged = true;
    save_stage(&parent1, work_dir).expect("Should save parent 1");

    let mut parent2 = create_test_stage("parent-2", "Parent 2", StageStatus::Executing);
    parent2.try_complete(None).unwrap();
    parent2.merged = false; // Not yet merged
    save_stage(&parent2, work_dir).expect("Should save parent 2");

    // Create child depending on both
    let mut child = create_test_stage("child-stage", "Child Stage", StageStatus::WaitingForDeps);
    child.add_dependency("parent-1".to_string());
    child.add_dependency("parent-2".to_string());
    save_stage(&child, work_dir).expect("Should save child stage");

    // Child deps NOT satisfied (parent-2 not merged)
    let satisfied =
        are_all_dependencies_satisfied(&child, work_dir).expect("Should check dependencies");
    assert!(
        !satisfied,
        "Dependencies not satisfied when one parent not merged"
    );

    let triggered = trigger_dependents("parent-2", work_dir).expect("Should trigger dependents");
    assert!(triggered.is_empty(), "No stages should be triggered");

    // Now merge parent-2
    let mut parent2 = load_stage("parent-2", work_dir).expect("Should load parent-2");
    parent2.merged = true;
    save_stage(&parent2, work_dir).expect("Should save parent-2 with merged=true");

    // Now child deps should be satisfied
    let child = load_stage("child-stage", work_dir).expect("Should reload child");
    let satisfied =
        are_all_dependencies_satisfied(&child, work_dir).expect("Should check dependencies");
    assert!(
        satisfied,
        "Dependencies should be satisfied when all parents completed and merged"
    );

    let triggered = trigger_dependents("parent-2", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1, "Child should be triggered");
    assert_eq!(triggered[0], "child-stage");
}

/// Test: MergeBlocked status is retryable and doesn't satisfy dependencies.
#[test]
fn test_merge_blocked_status_behavior() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create stage in Executing status
    let stage = create_test_stage("test-stage", "Test Stage", StageStatus::Executing);
    save_stage(&stage, work_dir).expect("Should save stage");

    // Create dependent
    let mut dependent = create_test_stage(
        "dependent-stage",
        "Dependent Stage",
        StageStatus::WaitingForDeps,
    );
    dependent.add_dependency("test-stage".to_string());
    save_stage(&dependent, work_dir).expect("Should save dependent");

    // Transition to MergeBlocked (simulating merge failure)
    let mut stage = load_stage("test-stage", work_dir).expect("Should load stage");
    stage
        .try_mark_merge_blocked()
        .expect("Should transition to MergeBlocked");
    save_stage(&stage, work_dir).expect("Should save MergeBlocked stage");

    // Verify status
    let stage = load_stage("test-stage", work_dir).expect("Should reload stage");
    assert_eq!(stage.status, StageStatus::MergeBlocked);

    // Dependents NOT triggered
    let triggered = trigger_dependents("test-stage", work_dir).expect("Should trigger dependents");
    assert!(
        triggered.is_empty(),
        "MergeBlocked should not trigger dependents"
    );

    // Verify MergeBlocked is retryable to Executing
    let mut stage = load_stage("test-stage", work_dir).expect("Should reload stage");
    assert!(stage.status.can_transition_to(&StageStatus::Executing));
    stage
        .try_mark_executing()
        .expect("Should retry to Executing");
    assert_eq!(stage.status, StageStatus::Executing);
}

/// Test: Stages with no dependencies are immediately satisfiable.
#[test]
fn test_stage_with_no_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create a stage with no dependencies
    let stage = create_test_stage(
        "standalone",
        "Standalone Stage",
        StageStatus::WaitingForDeps,
    );
    save_stage(&stage, work_dir).expect("Should save standalone stage");

    // Verify dependencies are satisfied (empty deps = satisfied)
    let satisfied =
        are_all_dependencies_satisfied(&stage, work_dir).expect("Should check dependencies");
    assert!(
        satisfied,
        "Stage with no dependencies should have satisfied deps"
    );
}

/// Test: Chain of dependencies - only immediate dependents are triggered.
#[test]
fn test_dependency_chain() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create a chain: A -> B -> C
    let stage_a = create_test_stage("stage-a", "Stage A", StageStatus::Executing);
    save_stage(&stage_a, work_dir).expect("Should save stage A");

    let mut stage_b = create_test_stage("stage-b", "Stage B", StageStatus::WaitingForDeps);
    stage_b.add_dependency("stage-a".to_string());
    save_stage(&stage_b, work_dir).expect("Should save stage B");

    let mut stage_c = create_test_stage("stage-c", "Stage C", StageStatus::WaitingForDeps);
    stage_c.add_dependency("stage-b".to_string());
    save_stage(&stage_c, work_dir).expect("Should save stage C");

    // Complete stage A with merged=true
    let mut stage_a = load_stage("stage-a", work_dir).expect("Should load A");
    stage_a.try_complete(None).unwrap();
    stage_a.merged = true;
    save_stage(&stage_a, work_dir).expect("Should save completed A");

    // Only B should be triggered (not C - it depends on B which is not complete)
    let triggered = trigger_dependents("stage-a", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1, "Only B should be triggered");
    assert_eq!(triggered[0], "stage-b");

    // C should still be WaitingForDeps
    let stage_c = load_stage("stage-c", work_dir).expect("Should load C");
    assert_eq!(stage_c.status, StageStatus::WaitingForDeps);

    // Now complete B
    let mut stage_b = load_stage("stage-b", work_dir).expect("Should load B");
    stage_b.try_mark_executing().unwrap();
    stage_b.try_complete(None).unwrap();
    stage_b.merged = true;
    save_stage(&stage_b, work_dir).expect("Should save completed B");

    // Now C should be triggered
    let triggered = trigger_dependents("stage-b", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 1, "C should be triggered");
    assert_eq!(triggered[0], "stage-c");

    let stage_c = load_stage("stage-c", work_dir).expect("Should reload C");
    assert_eq!(stage_c.status, StageStatus::Queued);
}
