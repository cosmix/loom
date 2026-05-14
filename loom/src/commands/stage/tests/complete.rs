//! Tests for complete command

use super::super::complete::{complete, require_admin_capability};
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::models::stage::{StageStatus, StageType};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::load_stage;
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

/// Test helper: write `<runtime>/loom/admin.token` with the given content.
fn write_admin_token(runtime_dir: &Path, content: &str) {
    let loom_dir = runtime_dir.join("loom");
    std::fs::create_dir_all(&loom_dir).unwrap();
    std::fs::write(loom_dir.join("admin.token"), content).unwrap();
}

/// Test helper: capture XDG_RUNTIME_DIR, set it to `value`, and return the
/// previous value so the test can restore it on completion.
fn set_xdg_runtime_dir(value: Option<&Path>) -> Option<std::ffi::OsString> {
    let prev = std::env::var_os("XDG_RUNTIME_DIR");
    match value {
        Some(p) => std::env::set_var("XDG_RUNTIME_DIR", p),
        None => std::env::remove_var("XDG_RUNTIME_DIR"),
    }
    prev
}

fn restore_xdg_runtime_dir(prev: Option<std::ffi::OsString>) {
    match prev {
        Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
        None => std::env::remove_var("XDG_RUNTIME_DIR"),
    }
}

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

    // --no-verify now requires the host admin.token. Provide one for this
    // test so it reaches the zero-commits-ahead guard.
    let prev_xdg = set_xdg_runtime_dir(Some(temp_dir.path()));
    write_admin_token(temp_dir.path(), "admin-secret-token");

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
    restore_xdg_runtime_dir(prev_xdg);

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

    // --no-verify now requires the host admin.token. Provide one for this
    // test so it reaches the (non-bypass) completion path.
    let prev_xdg = set_xdg_runtime_dir(Some(temp_dir.path()));
    write_admin_token(temp_dir.path(), "admin-secret-token");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("test-stage".to_string(), None, true, false, false);

    std::env::set_current_dir(original_dir).unwrap();
    restore_xdg_runtime_dir(prev_xdg);

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
fn no_verify_succeeds_with_admin_token_at_host_path() {
    // When XDG_RUNTIME_DIR/loom/admin.token exists, require_admin_capability
    // must succeed. This guards the gate's pass-through for legitimate
    // host-operator use of --no-verify / --force-unsafe / --assume-merged.
    let tmp = TempDir::new().unwrap();
    let prev = set_xdg_runtime_dir(Some(tmp.path()));
    write_admin_token(tmp.path(), "admin-secret-token");

    let work_dir = tmp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir);

    restore_xdg_runtime_dir(prev);

    assert!(
        result.is_ok(),
        "require_admin_capability must pass when admin.token exists at host path: {:?}",
        result.err()
    );
}

#[test]
#[serial]
fn no_verify_rejected_when_admin_token_absent() {
    // When --no-verify is requested but admin.token is missing at the host
    // runtime path, require_admin_capability must fail closed with a clear
    // error mentioning the admin token. This is the structural guarantee
    // that container-resident agents cannot bypass acceptance.
    let tmp = TempDir::new().unwrap();
    let prev = set_xdg_runtime_dir(Some(tmp.path()));
    // No admin.token written.

    let work_dir = tmp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir);

    restore_xdg_runtime_dir(prev);

    assert!(
        result.is_err(),
        "require_admin_capability must fail when admin.token is absent"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("admin token") || err.contains("admin.token"),
        "expected error mentioning admin token, got: {err}"
    );
}

#[test]
#[serial]
fn force_unsafe_rejected_when_admin_token_absent() {
    // --force-unsafe path exercises the same gate. (The gate is flag-agnostic
    // at the check site — the caller in complete() decides when to call it
    // based on flag combinations. We assert the gate's behaviour here.)
    let tmp = TempDir::new().unwrap();
    let prev = set_xdg_runtime_dir(Some(tmp.path()));

    let work_dir = tmp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir);

    restore_xdg_runtime_dir(prev);

    assert!(
        result.is_err(),
        "force_unsafe path must reject when admin.token is absent"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("admin token") || err.contains("admin.token"),
        "expected error mentioning admin token, got: {err}"
    );
}

#[test]
#[serial]
fn assume_merged_rejected_when_admin_token_absent() {
    // --assume-merged path exercises the same gate.
    let tmp = TempDir::new().unwrap();
    let prev = set_xdg_runtime_dir(Some(tmp.path()));

    let work_dir = tmp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir);

    restore_xdg_runtime_dir(prev);

    assert!(
        result.is_err(),
        "assume_merged path must reject when admin.token is absent"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("admin token") || err.contains("admin.token"),
        "expected error mentioning admin token, got: {err}"
    );
}

#[test]
#[serial]
fn verify_path_succeeds_without_admin_token() {
    // When no_verify / force_unsafe / assume_merged are all false, complete()
    // must NOT invoke require_admin_capability. Run complete() without any
    // verification-bypass flags in a tempdir with no admin.token: the error
    // (if any) must NOT mention the admin token. Other failure modes (no git
    // repo, etc.) are acceptable — we only assert the gate did not fire.
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".work");

    // Redirect XDG_RUNTIME_DIR to an empty tempdir — no admin.token present.
    let xdg_tmp = TempDir::new().unwrap();
    let prev = set_xdg_runtime_dir(Some(xdg_tmp.path()));

    let mut stage = create_test_stage("verify-path-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete(
        "verify-path-stage".to_string(),
        None,
        false, // no_verify
        false, // force_unsafe
        false, // assume_merged
    );

    std::env::set_current_dir(original_dir).unwrap();
    restore_xdg_runtime_dir(prev);

    // We don't require Ok — the test setup has no real git repo and other
    // checks may fail. The critical invariant: the admin-token gate must
    // NOT fire when no bypass flag is set.
    if let Err(e) = result {
        let msg = format!("{:#}", e);
        assert!(
            !msg.contains("admin token") && !msg.contains("admin.token"),
            "complete() with no bypass flags must not invoke admin gate, got: {msg}"
        );
    }
}

#[test]
#[serial]
fn cli_without_admin_token_fails_admin_check() {
    // $XDG_RUNTIME_DIR is set but no loom/admin.token exists. The admin
    // gate must refuse the bypass flags. This is the structural defence
    // against an agent invoking --no-verify without daemon authority.
    let tmp = TempDir::new().unwrap();
    let prev = set_xdg_runtime_dir(Some(tmp.path()));
    // Deliberately do NOT write admin.token.

    let work_dir = tmp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir);

    restore_xdg_runtime_dir(prev);

    assert!(
        result.is_err(),
        "missing admin token must fail the admin gate"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("admin token") || err.contains("admin.token"),
        "expected error mentioning admin token, got: {err}"
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
