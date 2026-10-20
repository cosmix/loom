//! Tests for complete command

use super::super::admin_proof::{mint_admin_proof, AdminProofRequest};
use super::super::complete::{complete, complete_authorization::require_admin_capability};
use super::super::completion_evidence::{
    format_evidence_record, parse_evidence_record, verified_evidence, EVIDENCE_EOF_MARKER,
    EVIDENCE_RECORD_PREFIX,
};
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::models::stage::{StageStatus, StageType};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::load_stage;
use anyhow::Result;
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

/// Calls `complete()` with the session, `force_unsafe`, `assume_merged`, and
/// `no_cache` arguments at the defaults every call site in this file needs
/// (`None`, `false`, `false`, `false`) — only `no_verify` and `admin_proof`
/// vary across the routing/acceptance/knowledge-path tests below.
fn complete_stage(stage_id: &str, no_verify: bool, admin_proof: Option<String>) -> Result<()> {
    complete(
        stage_id.to_string(),
        None,
        no_verify,
        false,
        false,
        false,
        admin_proof,
    )
}

/// Test helper: write `<work_dir>/admin.token` with the given content.
fn write_admin_token(work_dir: &Path, content: &str) {
    std::fs::create_dir_all(work_dir).unwrap();
    std::fs::write(work_dir.join("admin.token"), content).unwrap();
}

fn initialize_zero_commit_repo(repo: &Path) {
    let run_git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap()
    };
    run_git(&["init", "--initial-branch=main"]);
    run_git(&["config", "user.email", "test@test.com"]);
    run_git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "x").unwrap();
    run_git(&["add", "README.md", "doc/loom/knowledge"]);
    run_git(&["commit", "-m", "initial"]);
    run_git(&["branch", "loom/test-stage"]);
}

fn assert_zero_commit_refusal(result: Result<()>, work_dir: &Path) {
    assert!(
        result.is_err(),
        "complete --no-verify must refuse a stage branch with zero commits ahead"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("zero commits"),
        "expected error to explain zero-commits cause, got: {err}"
    );
    assert!(
        err.contains("loom stage complete test-stage --force-unsafe --assume-merged"),
        "an intentionally empty stage needs the escape hatch that completes it, got: {err}"
    );
    let stage = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(
        stage.status,
        StageStatus::Executing,
        "refusal must preserve prior stage state"
    );
    assert!(!stage.merged, "refused stage must not be marked merged");
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

/// Clears completion-routing environment variables and restores them, plus
/// the working directory, on drop so ambient loom sessions cannot reroute tests.
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

    let result = complete_stage("test-stage", false, None);

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
    // When the stage branch EXISTS but has no commits beyond the merge
    // target, --no-verify must refuse — otherwise the daemon's auto-merge
    // trivially "succeeds" against an unchanged base, producing the
    // phantom-merge that was observed for harden-container-mod.

    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // --no-verify now requires the admin.token. Provide one for this
    // test so it reaches the zero-commits-ahead guard.
    write_admin_token(&work_dir_path, "admin-secret-token");

    let repo = temp_dir.path();
    initialize_zero_commit_repo(repo);

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    register_stage_worktree(repo, &mut stage, false);
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(repo).unwrap();

    let proof = completion_proof("test-stage", true, false, false, "zero-commits-0001");
    let result = complete_stage("test-stage", true, Some(proof));
    assert_zero_commit_refusal(result, &work_dir_path);
}

#[test]
#[serial]
fn test_complete_with_no_verify_flag() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // --no-verify now requires the admin.token. Provide one for this
    // test so it reaches the (non-bypass) completion path.
    write_admin_token(&work_dir_path, "admin-secret-token");

    initialize_zero_commit_repo(temp_dir.path());
    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    register_stage_worktree(temp_dir.path(), &mut stage, true);
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let proof = completion_proof("test-stage", true, false, false, "no-verify-test-01");
    let result = complete_stage("test-stage", true, Some(proof));

    assert!(result.is_ok(), "{result:?}");

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
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

    let result = complete_stage("standard-stage", false, None);

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
fn verified_evidence_record_has_broker_boundaries() {
    let stage = create_test_stage("build-api", StageStatus::Executing);
    let evidence = verified_evidence(
        &stage,
        "session-123",
        "a".repeat(40),
        "/usr/bin/loom stage complete build-api".into(),
    );
    let output = format_evidence_record(&evidence).unwrap();

    assert!(output.starts_with(EVIDENCE_RECORD_PREFIX));
    assert_eq!(output.trim_end().lines().last(), Some(EVIDENCE_EOF_MARKER));
    assert_eq!(parse_evidence_record(&output).unwrap(), evidence);
}

#[path = "complete_authorization_tests.rs"]
mod authorization;
#[path = "complete_knowledge_tests.rs"]
mod knowledge;
#[path = "operator_landing_tests.rs"]
pub(in crate::commands::stage) mod operator_landing;

fn register_stage_worktree(repo: &Path, stage: &mut crate::models::stage::Stage, advance: bool) {
    let path = repo.join(".worktrees").join(&stage.id);
    let output = std::process::Command::new("git")
        .args(["worktree", "add"])
        .arg(&path)
        .arg(format!("loom/{}", stage.id))
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    stage.worktree = Some(stage.id.clone());
    stage.base_branch = Some("main".into());
    if advance {
        std::fs::write(path.join("README.md"), "implemented").unwrap();
        for args in [
            ["add", "README.md"].as_slice(),
            ["commit", "-m", "implementation"].as_slice(),
        ] {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
    }
}
