//! Tests for complete command

use super::super::admin_proof::{mint_admin_proof, AdminProofRequest};
use super::super::complete::{
    complete, complete_authorization::require_admin_capability, verification_passed_marker_line,
};
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::models::stage::{StageStatus, StageType};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::load_stage;
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

/// Test helper: write `<work_dir>/admin.token` with the given content.
fn write_admin_token(work_dir: &Path, content: &str) {
    std::fs::create_dir_all(work_dir).unwrap();
    std::fs::write(work_dir.join("admin.token"), content).unwrap();
}

fn completion_proof(
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    nonce: &str,
) -> String {
    mint_admin_proof(
        "admin-secret-token",
        AdminProofRequest::completion(stage_id, no_verify, force_unsafe, assume_merged),
        nonce,
    )
}

/// Clears `LOOM_STAGE_ID`/`LOOM_SESSION_ID`/`LOOM_WORKTREE_PATH` and restores
/// them, plus the working directory, on drop. Mirrors the shape of the
/// `EnvGuard` in `commands/memory/handlers/tests.rs`.
///
/// `complete()` routes through `sandbox_control_session`
/// (`control_session.rs`), which reads these three vars from the ambient
/// process environment. This suite commonly runs INSIDE a loom worktree
/// session, which leaves them set for the orchestrator session that spawned
/// this test binary — so without clearing them, `complete()` in these tests
/// would silently take the SANDBOXED worktree-completion route instead of the
/// ordinary host-side one they mean to exercise, and fail for reasons
/// unrelated to what they assert.
///
/// Restoring the cwd on `Drop` (rather than a manual call placed after the
/// call under test) means a panicking `complete()` call — the one case
/// `#[serial]` cannot protect against, since the leaked cwd outlives the
/// failing test — still restores it for every test that runs after it in
/// this binary.
struct EnvGuard {
    original_dir: std::path::PathBuf,
    original_stage_id: Option<String>,
    original_session_id: Option<String>,
    original_worktree_path: Option<String>,
}

impl EnvGuard {
    fn new() -> Self {
        let guard = Self {
            original_dir: std::env::current_dir().unwrap(),
            original_stage_id: std::env::var("LOOM_STAGE_ID").ok(),
            original_session_id: std::env::var("LOOM_SESSION_ID").ok(),
            original_worktree_path: std::env::var("LOOM_WORKTREE_PATH").ok(),
        };
        std::env::remove_var("LOOM_STAGE_ID");
        std::env::remove_var("LOOM_SESSION_ID");
        std::env::remove_var("LOOM_WORKTREE_PATH");
        guard
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.original_dir).unwrap();
        restore_var("LOOM_STAGE_ID", &self.original_stage_id);
        restore_var("LOOM_SESSION_ID", &self.original_session_id);
        restore_var("LOOM_WORKTREE_PATH", &self.original_worktree_path);
    }
}

fn restore_var(name: &str, value: &Option<String>) {
    match value {
        Some(v) => std::env::set_var(name, v),
        None => std::env::remove_var(name),
    }
}

#[test]
#[serial]
fn test_complete_with_passing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete("test-stage".to_string(), None, false, false, false, None);

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
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // --no-verify now requires the admin.token. Provide one for this
    // test so it reaches the zero-commits-ahead guard.
    write_admin_token(&work_dir_path, "admin-secret-token");

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

    let _guard = EnvGuard::new();
    std::env::set_current_dir(repo).unwrap();

    let proof = completion_proof("test-stage", true, false, false, "zero-commits-0001");
    let result = complete(
        "test-stage".to_string(),
        None,
        true,
        false,
        false,
        Some(proof),
    );

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
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // --no-verify now requires the admin.token. Provide one for this
    // test so it reaches the (non-bypass) completion path.
    write_admin_token(&work_dir_path, "admin-secret-token");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let proof = completion_proof("test-stage", true, false, false, "no-verify-test-01");
    let result = complete(
        "test-stage".to_string(),
        None,
        true,
        false,
        false,
        Some(proof),
    );

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_sets_merged_true() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage (no acceptance criteria)
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete(
        "knowledge-stage".to_string(),
        None,
        false,
        false,
        false,
        None,
    );

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
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage with passing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete(
        "knowledge-stage".to_string(),
        None,
        false,
        false,
        false,
        None,
    );

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
    assert!(loaded_stage.merged);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_with_failing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage with failing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete(
        "knowledge-stage".to_string(),
        None,
        false,
        false,
        false,
        None,
    );

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
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage
    let mut knowledge_stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    knowledge_stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &knowledge_stage);

    // Create a dependent stage waiting for the knowledge stage
    let mut dependent_stage = create_test_stage("dependent-stage", StageStatus::WaitingForDeps);
    dependent_stage.dependencies = vec!["knowledge-stage".to_string()];
    save_test_stage(&work_dir_path, &dependent_stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete(
        "knowledge-stage".to_string(),
        None,
        false,
        false,
        false,
        None,
    );

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
fn no_verify_succeeds_with_matching_one_time_proof() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    write_admin_token(&work_dir, "admin-secret-token");

    let proof = completion_proof("test-stage", true, false, false, "admin-gate-test1");
    let result =
        require_admin_capability(&work_dir, "test-stage", true, false, false, Some(&proof));

    assert!(
        result.is_ok(),
        "require_admin_capability must pass when admin.token exists: {:?}",
        result.err()
    );
}

#[test]
#[serial]
fn no_verify_rejected_when_operator_proof_absent() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    // No admin.token written.

    let result = require_admin_capability(&work_dir, "test-stage", true, false, false, None);

    assert!(
        result.is_err(),
        "require_admin_capability must fail when its caller supplies no proof"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
    );
}

#[test]
#[serial]
fn force_unsafe_rejected_when_operator_proof_absent() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir, "test-stage", false, true, false, None);

    assert!(
        result.is_err(),
        "force_unsafe path must reject when the proof is absent"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
    );
}

#[test]
#[serial]
fn assume_merged_rejected_when_operator_proof_absent() {
    // --assume-merged path exercises the same gate.
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir, "test-stage", false, true, true, None);

    assert!(
        result.is_err(),
        "assume_merged path must reject when the proof is absent"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
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
    let work_dir_path = temp_dir.path().join(".loom").join("work");
    // No admin.token present in the work dir.

    let mut stage = create_test_stage("verify-path-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete(
        "verify-path-stage".to_string(),
        None,
        false, // no_verify
        false, // force_unsafe
        false, // assume_merged
        None,  // admin_proof
    );

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
fn cli_without_operator_proof_fails_admin_check() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    // Deliberately do NOT write admin.token.

    let result = require_admin_capability(&work_dir, "test-stage", true, false, false, None);

    assert!(
        result.is_err(),
        "missing caller proof must fail the admin gate"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
    );
}

#[test]
#[serial]
fn test_complete_standard_stage_not_routed_to_knowledge() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a standard stage (default stage_type)
    let mut stage = create_test_stage("standard-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    // Ensure it's explicitly standard (default)
    stage.stage_type = StageType::Standard;
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete(
        "standard-stage".to_string(),
        None,
        false,
        false,
        false,
        None,
    );

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

#[test]
fn verification_passed_marker_line_matches_the_bridges_exact_match() {
    // `hooks/loom-control-complete.sh` builds its own copy of this exact
    // string (`MARKER="LOOM_CONTROL_VERIFICATION_PASSED stage=$STAGE_ID
    // session=$SESSION_ID"`) and matches it as an exact whole line of
    // stdout before it will forward completion to the daemon. This test
    // pins the Rust side's format so a later "improve the wording" edit to
    // `run_verification_phase` fails here instead of silently breaking
    // completion for every sandboxed worktree session.
    assert_eq!(
        verification_passed_marker_line("build-api", "session-123"),
        "LOOM_CONTROL_VERIFICATION_PASSED stage=build-api session=session-123"
    );
}
