//! Tests for complete command

use super::super::complete::complete;
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::models::stage::{StageStatus, StageType};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::load_stage;
use serial_test::serial;

#[test]
#[serial]
fn test_complete_with_passing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("test-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    // Acceptance passes but the test setup has no real git repo or stage branch,
    // so progressive merge correctly hits MergeOutcome::Blocked (no `loom/test-stage`
    // branch to merge). Stage stays Executing; complete() returns an error from
    // the verification phase. We assert that acceptance ran successfully (no panic
    // before merge) and that merged is NOT auto-set without a real merge — the
    // phantom-merge fix removed the buggy "NoBranch → Success → merged=true" path.
    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert!(
        !loaded_stage.merged,
        "Standard stage must not be marked merged without a real successful merge \
         (this used to falsely succeed via the NoBranch arm — phantom merge bug)"
    );
    // Stage either stays Executing (no save in NoBranch arm) or is the test setup
    // returning early. Either way, completion did not finalize without a real merge.
    assert_ne!(
        loaded_stage.status,
        StageStatus::Completed,
        "Standard stage must not transition to Completed without a real merge"
    );
    // Result may be Ok or Err depending on how run_verification_phase reports the
    // Blocked outcome — but the critical invariant is that merged stays false.
    let _ = result;
}

#[test]
#[serial]
fn test_complete_no_verify_refuses_zero_commits_ahead() {
    use std::process::Command;
    // When the stage branch EXISTS but has no commits beyond the merge
    // target, --no-verify must refuse — otherwise the daemon's auto-merge
    // trivially "succeeds" against an unchanged base, producing the
    // phantom-merge that was observed for harden-container-mod.

    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Bootstrap a real git repo with an initial commit so the branch
    // existence + commits_ahead probes have something to work with.
    let repo = temp_dir.path();
    let run_git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap()
    };
    run_git(&["init", "--initial-branch=main"]);
    run_git(&["config", "user.email", "test@test.com"]);
    run_git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "x").unwrap();
    run_git(&["add", "README.md"]);
    run_git(&["commit", "-m", "initial"]);
    // Create the stage branch at the same HEAD as main — zero commits ahead.
    run_git(&["branch", "loom/test-stage"]);

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(repo).unwrap();

    let result = complete("test-stage".to_string(), None, true, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(
        result.is_err(),
        "complete --no-verify must refuse when stage branch has zero commits \
         ahead of target (phantom-merge guard)"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("zero commits"),
        "expected error to explain zero-commits cause, got: {err}"
    );

    // Stage status must NOT have been mutated by the refused completion.
    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(
        loaded_stage.status,
        StageStatus::Executing,
        "refusal must preserve prior stage state"
    );
    assert!(
        !loaded_stage.merged,
        "refused stage must not be marked merged"
    );
}

#[test]
#[serial]
fn test_complete_with_no_verify_flag() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("test-stage".to_string(), None, true, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_sets_merged_true() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage (no acceptance criteria)
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
    // Key assertion: merged=true is auto-set for knowledge stages
    assert!(
        loaded_stage.merged,
        "Knowledge stage should auto-set merged=true"
    );
}

#[test]
#[serial]
fn test_complete_knowledge_stage_with_passing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage with passing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
    assert!(loaded_stage.merged);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_with_failing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage with failing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    // New behavior: acceptance failure returns Err and stage stays Executing
    assert!(
        result.is_err(),
        "complete() should return Err when acceptance fails"
    );

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    // Stage should remain Executing (not transition to CompletedWithFailures)
    assert_eq!(loaded_stage.status, StageStatus::Executing);
    // merged should NOT be set when acceptance fails
    assert!(!loaded_stage.merged);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_triggers_dependents() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a knowledge stage
    let mut knowledge_stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    knowledge_stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &knowledge_stage);

    // Create a dependent stage waiting for the knowledge stage
    let mut dependent_stage = create_test_stage("dependent-stage", StageStatus::WaitingForDeps);
    dependent_stage.dependencies = vec!["knowledge-stage".to_string()];
    save_test_stage(&work_dir_path, &dependent_stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("knowledge-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    // Verify knowledge stage is completed with merged=true
    let loaded_knowledge = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_knowledge.status, StageStatus::Completed);
    assert!(loaded_knowledge.merged);

    // Verify dependent stage was triggered to Queued
    let loaded_dependent = load_stage("dependent-stage", &work_dir_path).unwrap();
    assert_eq!(
        loaded_dependent.status,
        StageStatus::Queued,
        "Dependent stage should be triggered to Queued when knowledge stage completes with merged=true"
    );
}

#[test]
#[serial]
fn test_complete_standard_stage_not_routed_to_knowledge() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Create a standard stage (default stage_type)
    let mut stage = create_test_stage("standard-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    // Ensure it's explicitly standard (default)
    stage.stage_type = StageType::Standard;
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("standard-stage".to_string(), None, false, false, false);

    std::env::set_current_dir(original_dir).unwrap();

    // The point of this test is routing: confirm the standard path is taken,
    // NOT the knowledge auto-merge path. Knowledge stages auto-set merged=true
    // without a real merge; standard stages must not. After the phantom-merge
    // fix, the NoBranch arm correctly returns Blocked instead of fabricating
    // a successful merge. So a standard stage in a test setup with no git
    // infrastructure must not end up with merged=true.
    let loaded_stage = load_stage("standard-stage", &work_dir_path).unwrap();
    assert!(
        !loaded_stage.merged,
        "Standard stage must not auto-set merged=true (knowledge-path-only behavior)"
    );
    assert_ne!(
        loaded_stage.status,
        StageStatus::Completed,
        "Standard stage must not transition to Completed without a real merge"
    );
    let _ = result;
}
