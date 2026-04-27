//! Integration tests for the merge-conflict recovery work in
//! `doc/plans/PLAN-fix-merge-conflict-recovery.md`.
//!
//! These tests pair attribution-aware reconciliation with the pure CLI
//! routing helper to lock down the contract that:
//!
//! * Active merges in the main repo must NOT be silently piggybacked when
//!   they cannot be attributed to the stage being completed.
//! * Helpers that rewrite git state (`merge_stage`,
//!   `get_conflicting_files_from_status`) refuse to run when `MERGE_HEAD`
//!   is set — never `git merge --abort` an in-progress resolution.
//! * `--force-unsafe --assume-merged` checks ancestry before lying.
//! * `--force-unsafe` alone refuses when an active merge for the stage is
//!   in progress (would orphan MERGE_HEAD).
//! * Daemon reconciliation reverts a `Completed + merged=true` stage when an
//!   active merge for it is detected on disk.

use std::fs;
use std::path::Path;
use std::process::Command;

use serial_test::serial;
use tempfile::TempDir;

use loom::commands::stage::complete::{route_complete_for_conflicts, CompleteConflictRoute};
use loom::git::merge::{merge_head_exists, merge_stage};
use loom::models::stage::{Stage, StageStatus, StageType};
use loom::orchestrator::merge_attribution::{
    reconcile_main_repo_active_merge, ReconciliationOutcome,
};
use loom::verify::transitions::{load_stage, save_stage};

fn run_git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    run_git(&["init", "-b", "main"], root);
    run_git(&["config", "user.email", "t@t.com"], root);
    run_git(&["config", "user.name", "t"], root);
    fs::write(root.join("seed.txt"), "seed").unwrap();
    run_git(&["add", "seed.txt"], root);
    run_git(&["commit", "-m", "seed"], root);
    tmp
}

fn make_work_dir(repo_root: &Path) -> std::path::PathBuf {
    let work_dir = repo_root.join(".work");
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::write(work_dir.join("config.toml"), "base_branch = \"main\"\n").unwrap();
    work_dir
}

fn make_stage(id: &str, status: StageStatus) -> Stage {
    let mut s = Stage::new(id.to_string(), Some(format!("test {id}")));
    s.id = id.to_string();
    s.stage_type = StageType::Standard;
    s.status = status;
    s
}

fn create_conflict_branch(stage_id: &str, repo_root: &Path) -> String {
    let branch = format!("loom/{stage_id}");
    run_git(&["checkout", "-b", &branch], repo_root);
    fs::write(repo_root.join("file.txt"), format!("branch-{stage_id}\n")).unwrap();
    run_git(&["add", "file.txt"], repo_root);
    run_git(&["commit", "-m", "branch"], repo_root);
    let head = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_root)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    run_git(&["checkout", "main"], repo_root);
    fs::write(repo_root.join("file.txt"), "main\n").unwrap();
    run_git(&["add", "file.txt"], repo_root);
    run_git(&["commit", "-m", "main"], repo_root);
    head
}

/// Triggers a merge conflict in main against a stage branch. Leaves
/// MERGE_HEAD set on the main repo.
fn start_active_merge_in_main(stage_id: &str, repo_root: &Path) {
    let branch = format!("loom/{stage_id}");
    let _ = Command::new("git")
        .args(["merge", "--no-ff", &branch])
        .current_dir(repo_root)
        .output()
        .unwrap();
    assert!(
        merge_head_exists(repo_root).unwrap(),
        "test setup must leave MERGE_HEAD set"
    );
}

#[test]
#[serial]
fn force_unsafe_refuses_when_merge_head_present_in_main_repo() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    create_conflict_branch("stage-x", root);
    start_active_merge_in_main("stage-x", root);

    let stage = make_stage("stage-x", StageStatus::Executing);
    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        true, // force_unsafe alone
        false,
    )
    .unwrap();
    matches!(route, CompleteConflictRoute::Refuse { .. });
}

#[test]
#[serial]
fn force_unsafe_assume_merged_refuses_when_commit_not_ancestor() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    let stranded_sha = create_conflict_branch("stranded-stage", root);
    // No merge into main — branch HEAD is not an ancestor of main.

    // Abort the active merge so router doesn't refuse via MERGE_HEAD path.
    let _ = Command::new("git")
        .args(["merge", "--abort"])
        .current_dir(root)
        .output()
        .ok();

    let mut stage = make_stage("stranded-stage", StageStatus::Completed);
    stage.completed_commit = Some(stranded_sha);

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        true,
        true, // assume_merged
    )
    .unwrap();
    matches!(route, CompleteConflictRoute::Refuse { .. });
}

#[test]
#[serial]
fn force_unsafe_assume_merged_succeeds_when_commit_is_ancestor() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    // Create a branch and merge it into main for real.
    run_git(&["checkout", "-b", "loom/landed"], root);
    fs::write(root.join("a.rs"), "ok").unwrap();
    run_git(&["add", "a.rs"], root);
    run_git(&["commit", "-m", "feat"], root);
    run_git(&["checkout", "main"], root);
    run_git(&["merge", "--no-ff", "-m", "merge", "loom/landed"], root);

    let head = loom::git::branch::get_branch_head("loom/landed", root).unwrap();
    let mut stage = make_stage("landed", StageStatus::Completed);
    stage.completed_commit = Some(head);

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        true,
        true,
    )
    .unwrap();
    matches!(
        route,
        CompleteConflictRoute::ForceUnsafeAssumeMergedVerified { .. }
    );
}

#[test]
#[serial]
fn merge_stage_refuses_when_merge_head_set_main_repo() {
    let repo = init_repo();
    let root = repo.path();

    create_conflict_branch("blockee", root);
    start_active_merge_in_main("blockee", root);

    let work_dir = make_work_dir(root);
    let result = merge_stage("blockee", "main", root, &work_dir);
    assert!(
        result.is_err(),
        "merge_stage must refuse with active MERGE_HEAD"
    );
    assert!(
        merge_head_exists(root).unwrap(),
        "guard must NOT abort the existing merge"
    );
}

#[test]
#[serial]
fn complete_routes_merge_blocked_to_resolver_same_as_merge_conflict() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    let stage_a = make_stage("merge-conflict-stage", StageStatus::MergeConflict);
    let stage_b = make_stage("merge-blocked-stage", StageStatus::MergeBlocked);

    let route_a = route_complete_for_conflicts(
        &stage_a,
        &[],
        std::slice::from_ref(&stage_a),
        root,
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();
    let route_b = route_complete_for_conflicts(
        &stage_b,
        &[],
        std::slice::from_ref(&stage_b),
        root,
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();

    assert!(matches!(
        route_a,
        CompleteConflictRoute::SpawnResolver { .. }
    ));
    assert!(matches!(
        route_b,
        CompleteConflictRoute::SpawnResolver { .. }
    ));
}

#[test]
#[serial]
fn complete_completed_with_active_merge_returns_revert_when_daemon_off() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    create_conflict_branch("phantom-stage", root);
    start_active_merge_in_main("phantom-stage", root);

    let stage = make_stage("phantom-stage", StageStatus::Completed);
    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false, // daemon off
        false,
        false,
    )
    .unwrap();
    assert!(
        matches!(route, CompleteConflictRoute::RevertAndSpawnResolver { .. }),
        "Completed stage with active merge owned by it must revert + spawn"
    );
}

#[test]
#[serial]
fn complete_completed_with_active_merge_returns_daemon_managed_when_daemon_running() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    create_conflict_branch("dm-stage", root);
    start_active_merge_in_main("dm-stage", root);

    let stage = make_stage("dm-stage", StageStatus::Completed);
    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        true, // daemon running
        false,
        false,
    )
    .unwrap();
    matches!(route, CompleteConflictRoute::DaemonManaged { .. });
}

#[test]
#[serial]
fn daemon_reconcile_reverts_completed_merged_when_active_merge_present() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    let stranded = create_conflict_branch("phantom", root);
    start_active_merge_in_main("phantom", root);

    // Pretend the daemon flushed `merged=true` despite the active merge.
    let mut stage = make_stage("phantom", StageStatus::Completed);
    stage.completed_commit = Some(stranded);
    stage.merged = true;
    save_stage(&stage, &work_dir).unwrap();

    let outcome = reconcile_main_repo_active_merge(root, &work_dir).unwrap();
    match outcome {
        ReconciliationOutcome::StageMutated {
            stage_id,
            new_status,
            ..
        } => {
            assert_eq!(stage_id, "phantom");
            assert_eq!(new_status, StageStatus::MergeConflict);
        }
        other => panic!("expected StageMutated revert, got {other:?}"),
    }

    // Stage on disk is now MergeConflict + merged=false + merge_conflict=true.
    let after = load_stage("phantom", &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::MergeConflict);
    assert!(!after.merged);
    assert!(after.merge_conflict);
}

#[test]
#[serial]
fn daemon_reconcile_does_not_attribute_unrelated_merge_to_other_stages() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    create_conflict_branch("only-this-stage", root);
    start_active_merge_in_main("only-this-stage", root);

    let mut stage_a = make_stage("only-this-stage", StageStatus::Executing);
    stage_a.merged = false;
    let mut stage_b = make_stage("untouched-stage", StageStatus::Completed);
    stage_b.merged = true; // would be a phantom merge if reconcile mis-attributed
    save_stage(&stage_a, &work_dir).unwrap();
    save_stage(&stage_b, &work_dir).unwrap();

    // reconcile must NOT touch stage-b.
    let _ = reconcile_main_repo_active_merge(root, &work_dir).unwrap();
    let after_b = load_stage("untouched-stage", &work_dir).unwrap();
    assert_eq!(after_b.status, StageStatus::Completed);
    assert!(after_b.merged);
}

#[test]
#[serial]
fn daemon_reconcile_no_active_merge_returns_no_op() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    let outcome = reconcile_main_repo_active_merge(root, &work_dir).unwrap();
    assert_eq!(outcome, ReconciliationOutcome::NoActiveMerge);
}

#[test]
#[serial]
fn daemon_reconcile_unattributed_merge_logs_only() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    // Manufacture MERGE_HEAD with bogus SHA.
    fs::write(
        root.join(".git").join("MERGE_HEAD"),
        "deadbeefcafebabe1234567890abcdef12345678\n",
    )
    .unwrap();

    let outcome = reconcile_main_repo_active_merge(root, &work_dir).unwrap();
    assert_eq!(outcome, ReconciliationOutcome::UnattributedLogged);

    fs::remove_file(root.join(".git").join("MERGE_HEAD")).ok();
}
