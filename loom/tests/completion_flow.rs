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

// =============================================================================
// Acceptance Criteria Directory Tests
//
// These tests verify that acceptance criteria are executed from the correct
// directory (worktree root) regardless of the current working directory.
// =============================================================================

/// Test: Acceptance criteria run from worktree root, not from subdirectory.
///
/// When a user runs `loom stage complete` from a subdirectory within a worktree,
/// the acceptance criteria should execute from the worktree root.
///
/// This test:
/// 1. Creates a temporary "worktree" directory structure
/// 2. Sets up a stage with an acceptance criterion that writes a marker file
/// 3. Simulates being in a subdirectory within the worktree
/// 4. Runs the acceptance criterion with the worktree root as working_dir
/// 5. Verifies the marker file is created at the worktree root, not the subdirectory
#[test]
fn test_acceptance_runs_from_worktree_root_not_subdirectory() {
    use loom::verify::run_acceptance;
    use std::fs;

    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Create a nested subdirectory structure (simulating deep/nested/ within a worktree)
    let subdir = worktree_root.join("deep").join("nested");
    fs::create_dir_all(&subdir).expect("Should create nested subdirectory");

    // Create a stage with acceptance criterion that creates a marker file
    // This criterion creates a file at the current working directory
    let mut stage = create_test_stage("test-stage", "Test Stage", StageStatus::Executing);
    stage.add_acceptance_criterion("touch marker.txt".to_string());

    // Run acceptance with working_dir set to worktree root
    // This simulates what happens when find_worktree_root_from_cwd() resolves the root
    let result = run_acceptance(&stage, Some(worktree_root)).expect("Should run acceptance");

    // Acceptance should pass
    assert!(
        result.all_passed(),
        "Acceptance criterion 'touch marker.txt' should pass"
    );

    // The marker file should be at the worktree root, NOT in the subdirectory
    let marker_at_root = worktree_root.join("marker.txt");
    let marker_at_subdir = subdir.join("marker.txt");

    assert!(
        marker_at_root.exists(),
        "Marker file should exist at worktree root: {}",
        marker_at_root.display()
    );
    assert!(
        !marker_at_subdir.exists(),
        "Marker file should NOT exist in subdirectory: {}",
        marker_at_subdir.display()
    );
}

/// Test: Acceptance criterion that requires Cargo.toml passes from worktree root.
///
/// This tests a common real-world scenario where acceptance criteria check for
/// files at the project root (like Cargo.toml). This criterion would fail if
/// run from a subdirectory that doesn't have Cargo.toml.
#[test]
fn test_acceptance_criterion_requiring_cargo_toml() {
    use loom::verify::run_acceptance;
    use std::fs;

    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Create Cargo.toml at worktree root (simulating a Rust project)
    fs::write(
        worktree_root.join("Cargo.toml"),
        "[package]\nname = \"test\"",
    )
    .expect("Should create Cargo.toml");

    // Create a subdirectory that does NOT have Cargo.toml
    let subdir = worktree_root.join("src").join("lib");
    fs::create_dir_all(&subdir).expect("Should create subdirectory");

    // Create a stage with acceptance criterion that checks for Cargo.toml
    let mut stage = create_test_stage("cargo-stage", "Cargo Stage", StageStatus::Executing);
    stage.add_acceptance_criterion("test -f Cargo.toml".to_string());

    // Run acceptance from worktree root - should pass
    let result_from_root =
        run_acceptance(&stage, Some(worktree_root)).expect("Should run acceptance");
    assert!(
        result_from_root.all_passed(),
        "Criterion 'test -f Cargo.toml' should pass when run from worktree root"
    );

    // Run acceptance from subdirectory (without proper worktree resolution) - would fail
    // This simulates the old buggy behavior where working_dir was cwd instead of worktree root
    let result_from_subdir =
        run_acceptance(&stage, Some(subdir.as_path())).expect("Should run acceptance");
    assert!(
        !result_from_subdir.all_passed(),
        "Criterion 'test -f Cargo.toml' should FAIL when run from subdirectory without Cargo.toml"
    );
}

/// Test: Multiple acceptance criteria all run from the same directory.
///
/// Verifies that when multiple criteria are defined, they all execute
/// from the worktree root, maintaining consistent state between them.
#[test]
fn test_multiple_acceptance_criteria_run_from_same_directory() {
    use loom::verify::run_acceptance;
    use std::fs;

    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Create a stage with multiple criteria that depend on each other
    let mut stage = create_test_stage("multi-stage", "Multi Stage", StageStatus::Executing);
    // First criterion creates a file
    stage.add_acceptance_criterion("echo 'test content' > sequential_test.txt".to_string());
    // Second criterion checks that file exists (would fail if run in different directory)
    stage.add_acceptance_criterion("test -f sequential_test.txt".to_string());
    // Third criterion checks content (would fail if file wasn't created in same directory)
    stage.add_acceptance_criterion("grep -q 'test content' sequential_test.txt".to_string());

    // Run acceptance from worktree root
    let result = run_acceptance(&stage, Some(worktree_root)).expect("Should run acceptance");

    assert!(
        result.all_passed(),
        "All sequential criteria should pass when run from same directory"
    );

    // Verify the file was created at the worktree root
    let test_file = worktree_root.join("sequential_test.txt");
    assert!(
        test_file.exists(),
        "Test file should exist at worktree root"
    );

    let content = fs::read_to_string(&test_file).expect("Should read test file");
    assert!(
        content.contains("test content"),
        "Test file should contain expected content"
    );
}

/// Test: find_worktree_root_from_cwd correctly resolves worktree root from nested path.
///
/// This unit test verifies the helper function that the complete command uses
/// to determine the correct working directory for acceptance criteria.
#[test]
fn test_find_worktree_root_from_cwd_with_real_structure() {
    use loom::git::worktree::find_worktree_root_from_cwd;
    use std::fs;
    use std::path::PathBuf;

    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create a .worktrees/test-stage structure
    let worktree_path = base.join(".worktrees").join("test-stage");
    let nested_path = worktree_path.join("src").join("lib").join("module");
    fs::create_dir_all(&nested_path).expect("Should create nested structure");

    // Test finding worktree root from deeply nested path
    let found_root = find_worktree_root_from_cwd(&nested_path);
    assert!(found_root.is_some(), "Should find worktree root");

    // The found root should end with the worktree directory name
    let found = found_root.unwrap();
    let expected_suffix: PathBuf = [".worktrees", "test-stage"].iter().collect();
    assert!(
        found
            .to_string_lossy()
            .ends_with(&expected_suffix.to_string_lossy().to_string()),
        "Found root {} should end with {:?}",
        found.display(),
        expected_suffix
    );

    // Test that non-worktree paths return None
    let regular_path = base.join("regular").join("path");
    fs::create_dir_all(&regular_path).expect("Should create regular path");
    let no_root = find_worktree_root_from_cwd(&regular_path);
    assert!(
        no_root.is_none(),
        "Should not find worktree root for regular path"
    );
}

/// Test: Acceptance criteria with $WORKTREE variable expansion.
///
/// Verifies that the $WORKTREE context variable is properly expanded
/// to the working directory path in acceptance criteria.
#[test]
fn test_acceptance_criteria_worktree_variable_expansion() {
    use loom::verify::run_acceptance;

    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Create a stage with acceptance criterion using $WORKTREE variable
    // Note: We use the shell-style variable syntax that loom expands
    let mut stage = create_test_stage(
        "worktree-var-stage",
        "Worktree Var Stage",
        StageStatus::Executing,
    );
    // This criterion should expand $WORKTREE to the working directory
    // Using format string to include literal braces in shell variable
    let criterion = format!("touch ${{WORKTREE}}/expanded_marker.txt");
    stage.add_acceptance_criterion(criterion);

    // Run acceptance with working_dir set to worktree root
    let result = run_acceptance(&stage, Some(worktree_root)).expect("Should run acceptance");

    assert!(
        result.all_passed(),
        "Acceptance criterion with $WORKTREE variable should pass"
    );

    // The marker file should be at the worktree root
    let marker_at_root = worktree_root.join("expanded_marker.txt");
    assert!(
        marker_at_root.exists(),
        "Marker file created with $WORKTREE variable should exist at worktree root"
    );
}

/// Test: Acceptance criteria with relative paths work correctly from worktree root.
///
/// Ensures that relative paths in acceptance criteria are resolved relative to
/// the worktree root, not the current working directory.
#[test]
fn test_acceptance_criteria_relative_paths() {
    use loom::verify::run_acceptance;
    use std::fs;

    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Create the expected directory structure at worktree root
    let src_dir = worktree_root.join("src");
    fs::create_dir_all(&src_dir).expect("Should create src directory");

    // Create a stage with acceptance criterion using relative path
    let mut stage = create_test_stage("relative-stage", "Relative Stage", StageStatus::Executing);
    // This criterion creates a file in a relative path from the working directory
    stage.add_acceptance_criterion("touch src/created_file.rs".to_string());

    // Run acceptance with working_dir set to worktree root
    let result = run_acceptance(&stage, Some(worktree_root)).expect("Should run acceptance");

    assert!(
        result.all_passed(),
        "Acceptance criterion with relative path should pass"
    );

    // The file should be created relative to worktree root
    let created_file = worktree_root.join("src").join("created_file.rs");
    assert!(
        created_file.exists(),
        "File created with relative path should exist at src/created_file.rs"
    );
}
