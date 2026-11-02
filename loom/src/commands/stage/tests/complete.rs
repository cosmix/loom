//! Tests for complete command

use super::super::admin_proof::{mint_admin_proof, AdminProofRequest};
use super::super::complete::complete_relay::{route, SessionCall};
use super::super::complete::{complete, complete_authorization::require_admin_capability};
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::models::stage::{StageStatus, StageType};
use crate::plan::schema::AcceptanceCriterion;
use crate::relay::emit::{RelayContext, RelayMode};
use crate::verify::transitions::load_stage;
use anyhow::Result;
use serial_test::serial;
use std::path::{Path, PathBuf};
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
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run_git(&["init", "--initial-branch=main"]);
    run_git(&["config", "user.email", "test@test.com"]);
    run_git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "x").unwrap();
    // `setup_work_dir` scaffolds the knowledge files, which a real project has
    // committed; completion refuses them as untracked inputs otherwise.
    run_git(&["add", "README.md", "doc"]);
    run_git(&["commit", "-m", "initial"]);
    run_git(&["branch", "loom/test-stage"]);
    crate::git::worktree::capture_main_identity(repo, &repo.join(".loom/work")).unwrap();
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
fn retry_signal_counts_execution_retries() {
    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".loom/work");
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();
    for (id, retries, force, expected, persisted) in [
        ("fresh", 0, false, 1, 1),
        ("recovered", 2, false, 3, 3),
        ("forced", 2, true, 3, 0),
    ] {
        let mut stage = create_test_stage(id, StageStatus::Blocked);
        stage.retry_count = retries;
        stage.stall_recoveries = 7;
        save_test_stage(&work_dir, &stage);
        super::super::skip_retry::retry(id.into(), force, Some("resume".into())).unwrap();
        let updated = load_stage(id, &work_dir).unwrap();
        let signal_path = work_dir
            .join("signals")
            .join(format!("{}.md", updated.session.unwrap()));
        let signal = std::fs::read_to_string(signal_path).unwrap();
        assert!(
            signal.contains(&format!("**Recovery Attempt**: #{expected}")),
            "{signal}"
        );
        assert_eq!(updated.retry_count, persisted);
        assert_eq!(updated.stall_recoveries, 0);
    }
}

#[test]
#[serial]
fn test_complete_with_passing_acceptance() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    initialize_zero_commit_repo(temp_dir.path());
    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    let marker_dir = TempDir::new().unwrap();
    let marker = marker_dir.path().join("acceptance-ran");
    stage.sandbox.command_confinement = Some(crate::models::stage::CommandConfinement::Inherit);
    stage.acceptance = vec![AcceptanceCriterion::Simple(format!(
        "touch '{}'",
        marker.display()
    ))];
    register_stage_worktree(temp_dir.path(), &mut stage, true);
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete_stage("test-stage", false, None);

    assert!(marker.exists(), "acceptance command must run: {result:?}");
    assert!(result.is_ok(), "completion should succeed: {result:?}");
    assert_eq!(
        load_stage("test-stage", &work_dir_path).unwrap().status,
        StageStatus::Completed
    );
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
fn test_complete_standard_stage_without_repository_fails() {
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

    let error = format!("{:#}", result.unwrap_err());
    assert!(error.contains("Stage has no assigned worktree"), "{error}");
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
}

/// An in-session `loom stage complete test-stage` run from `cwd`.
fn session_call<'a>(work_dir: &'a Path, cwd: &'a Path, privileged: bool) -> SessionCall<'a> {
    SessionCall {
        stage_id: "test-stage",
        session_id: None,
        privileged,
        no_cache: false,
        work_dir,
        cwd,
    }
}

/// A stage or knowledge session `session-1`'s relay context, scratch
/// directory `scratch`.
fn relay_context(scratch: PathBuf, session_type: &str, work_dir: &Path) -> RelayContext {
    RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir: scratch,
        stage_id: Some("test-stage".to_string()),
        session_type: Some(session_type.to_string()),
        worktree_path: None,
        work_dir: Some(work_dir.to_path_buf()),
    }
}

/// Route `call` under `mode`; returns the exit code and the printed lines.
fn routed(mode: RelayMode, call: &SessionCall<'_>) -> (Option<i32>, String) {
    let mut out = Vec::new();
    let code = route(mode, call, &mut out).unwrap();
    (code, String::from_utf8(out).unwrap())
}

#[test]
fn a_legacy_session_is_refused_and_pointed_at_the_operator_shell() {
    let temp = TempDir::new().unwrap();
    let (code, text) = routed(
        RelayMode::Legacy,
        &session_call(temp.path(), temp.path(), false),
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(text.starts_with("Host job: refused (complete, "), "{text}");
    assert!(
        text.contains("This session cannot relay completion (no LOOM_SCRATCH_DIR)."),
        "{text}"
    );
    assert!(text.contains("loom stage retry test-stage"), "{text}");
}

#[test]
fn a_session_completion_refuses_the_privileged_flags() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let scratch = temp.path().join("session-1");
    let context = relay_context(scratch.clone(), "stage", &work_dir);
    let call = session_call(&work_dir, temp.path(), true);
    let (code, text) = routed(RelayMode::Relay(context), &call);
    assert_eq!(code, Some(1), "{text}");
    assert!(text.starts_with("Host job: refused (complete, "), "{text}");
    assert!(
        text.contains("--no-verify, --force-unsafe and --assume-merged are refused"),
        "{text}"
    );
    assert!(!scratch.exists(), "no ticket was written");
}

#[path = "complete_authorization_tests.rs"]
mod authorization;
#[path = "force_complete_race_tests.rs"]
mod force_complete_race;
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
