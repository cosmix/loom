//! The daemon's handling of a stage merge around the resolver: the conflict
//! and block outcomes of auto-merge, the per-tick retry of a blocked merge,
//! and what the resolver's exit does. Each test runs against a repository
//! with a real stage worktree; no resolver process is ever spawned.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::super::resolver_attempts::{attempts_dir, attempts_file, merge_resolver_attempts};
use super::super::resolver_spawn::test_fixtures::{
    git_ok, orchestrator_with_conflict, repo_with_stage_branches,
};
use super::Landing;
use crate::fs::stage_files::find_stage_file;
use crate::git::signing::tests::fake_signer;
use crate::git::{MergeBlock, StashReapply};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::Orchestrator;
use crate::verify::transitions::{load_stage, update_stage};

const ID: &str = "s";

/// A repository whose stage `ID` has a worktree on `loom/s` with one commit,
/// and an orchestrator over it holding the stage as `Completed`.
fn worktree_stage() -> (TempDir, Orchestrator) {
    let repo = repo_with_stage_branches(&[]);
    let worktree = repo.path().join(".worktrees").join(ID);
    let path = worktree.to_str().unwrap();
    git_ok(
        repo.path(),
        &["worktree", "add", "-q", "-b", "loom/s", path],
    );
    commit_file(&worktree, "stage.txt", "stage");
    let orchestrator = orchestrator_with_conflict(repo.path(), ID);
    set_status(&orchestrator, StageStatus::Completed);
    (repo, orchestrator)
}

fn worktree(repo: &TempDir) -> PathBuf {
    repo.path().join(".worktrees").join(ID)
}

fn commit_file(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git_ok(dir, &["add", name]);
    git_ok(dir, &["commit", "-q", "-m", name]);
}

fn set_status(orchestrator: &Orchestrator, status: StageStatus) {
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.status = status;
        Ok(())
    })
    .unwrap();
}

/// The stage in `MergeConflict` with the worktree's current head recorded as
/// its `completed_commit`, as the conflict entry does.
fn enter_conflict(orchestrator: &Orchestrator, repo: &TempDir) -> String {
    let head =
        crate::git::runner::run_git_checked(&["rev-parse", "HEAD"], &worktree(repo)).unwrap();
    let recorded = head.clone();
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.status = StageStatus::MergeConflict;
        stage.completed_commit = Some(recorded.clone());
        Ok(())
    })
    .unwrap();
    head
}

fn on_disk(orchestrator: &Orchestrator) -> Stage {
    load_stage(ID, &orchestrator.config.work_dir).unwrap()
}

fn stage_file_text(orchestrator: &Orchestrator) -> String {
    let stages = orchestrator.config.work_dir.join("stages");
    std::fs::read_to_string(find_stage_file(&stages, ID).unwrap().unwrap()).unwrap()
}

fn main_tip(repo: &TempDir) -> String {
    crate::git::runner::run_git_checked(&["rev-parse", "main"], repo.path()).unwrap()
}

/// `main` and the worktree both edit `seed.txt`: merging them conflicts.
fn make_conflict(repo: &TempDir) {
    commit_file(&worktree(repo), "seed.txt", "stage side");
    commit_file(repo.path(), "seed.txt", "main side");
}

/// An untracked `stage.txt` in the main checkout blocks the merge.
fn put_blocker(repo: &TempDir) -> PathBuf {
    let blocker = repo.path().join("stage.txt");
    std::fs::write(&blocker, "operator's own file").unwrap();
    blocker
}

/// `main` moves on and the worktree merges it, as a resolver does.
fn resolve_in_worktree(repo: &TempDir) {
    commit_file(repo.path(), "m.txt", "main");
    git_ok(
        &worktree(repo),
        &["merge", "-q", "main", "-m", "merge main"],
    );
}

#[test]
fn an_auto_merge_conflict_spawns_nothing_and_the_spawn_pass_counts_the_first_resolver() {
    let (repo, mut orchestrator) = worktree_stage();
    make_conflict(&repo);

    assert!(!orchestrator.try_auto_merge(ID));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(stage.merge.block, None);
    assert!(orchestrator.active_sessions.is_empty());
    let work_dir = &orchestrator.config.work_dir;
    assert_eq!(merge_resolver_attempts(work_dir, ID), 0);
    // The pass reserves attempt 1 before any spawn; a counter that cannot be
    // written sends the stage to review instead of spawning an uncounted one.
    std::fs::write(attempts_dir(work_dir), "not a dir").unwrap();
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    let reviewed = on_disk(&orchestrator);
    assert_eq!(reviewed.status, StageStatus::NeedsHumanReview);
    assert!(reviewed
        .review_reason
        .unwrap()
        .contains("could not be recorded"));
}

#[test]
fn an_auto_merge_block_is_persisted_and_gets_no_resolver() {
    let (repo, mut orchestrator) = worktree_stage();
    put_blocker(&repo);

    assert!(!orchestrator.try_auto_merge(ID));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    assert!(matches!(
        stage.merge.block,
        Some(MergeBlock::UncommittedOverlap { .. })
    ));
    assert!(!stage.merged);
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    let work_dir = &orchestrator.config.work_dir;
    assert!(!attempts_file(work_dir, ID).exists());
    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);
}

#[test]
fn an_unchanged_block_does_not_rewrite_the_stage_file() {
    let (repo, mut orchestrator) = worktree_stage();
    put_blocker(&repo);
    assert!(!orchestrator.try_auto_merge(ID));
    let before = stage_file_text(&orchestrator);

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    assert_eq!(stage_file_text(&orchestrator), before);
}

#[test]
fn the_retry_lands_a_cleared_block_and_removes_the_worktree() {
    let (repo, mut orchestrator) = worktree_stage();
    let blocker = put_blocker(&repo);
    assert!(!orchestrator.try_auto_merge(ID));
    std::fs::remove_file(blocker).unwrap();

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(stage.merge.block, None);
    assert!(!worktree(&repo).exists(), "no resolver runs in it");
}

#[test]
fn the_retry_keeps_the_worktree_while_a_resolver_is_tracked() {
    let (repo, mut orchestrator) = worktree_stage();
    let blocker = put_blocker(&repo);
    assert!(!orchestrator.try_auto_merge(ID));
    std::fs::remove_file(blocker).unwrap();
    let resolver = Session::new_merge("loom/s".to_string(), "main".to_string());
    orchestrator
        .active_sessions
        .insert(ID.to_string(), resolver);

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    assert!(on_disk(&orchestrator).merged);
    assert!(worktree(&repo).is_dir(), "the resolver's exit removes it");
}

#[test]
fn a_blocked_merge_that_now_conflicts_becomes_a_merge_conflict() {
    let (repo, mut orchestrator) = worktree_stage();
    put_blocker(&repo);
    assert!(!orchestrator.try_auto_merge(ID));
    make_conflict(&repo);

    let landing = orchestrator.land_stage_merge(ID, "main");

    assert_eq!(landing, Landing::Conflict(vec!["seed.txt".to_string()]));
    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(stage.merge.block, None);
}

#[test]
fn the_resolver_exit_removes_the_worktree_of_a_merge_already_landed() {
    let (repo, mut orchestrator) = worktree_stage();
    set_status(&orchestrator, StageStatus::MergeConflict);
    resolve_in_worktree(&repo);
    assert_eq!(orchestrator.land_stage_merge(ID, "main"), Landing::Merged);
    assert!(worktree(&repo).is_dir());

    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();

    assert!(!worktree(&repo).exists());
    assert!(on_disk(&orchestrator).merged);
}

#[test]
fn the_resolver_exit_lands_a_resolved_worktree_and_cleans_up() {
    let (repo, mut orchestrator) = worktree_stage();
    enter_conflict(&orchestrator, &repo);
    resolve_in_worktree(&repo);

    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert!(!worktree(&repo).exists());
}

#[test]
fn the_resolver_exit_without_a_recorded_commit_merges_nothing() {
    let (repo, mut orchestrator) = worktree_stage();
    set_status(&orchestrator, StageStatus::MergeConflict);
    resolve_in_worktree(&repo);
    let main_before = main_tip(&repo);

    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(!stage.merged);
    assert!(worktree(&repo).is_dir());
    assert_eq!(main_tip(&repo), main_before);
}

#[test]
fn the_resolver_exit_refuses_a_worktree_that_dropped_the_recorded_commit() {
    let (repo, mut orchestrator) = worktree_stage();
    enter_conflict(&orchestrator, &repo);
    commit_file(repo.path(), "m.txt", "main moved on");
    git_ok(&worktree(&repo), &["reset", "-q", "--hard", "main"]);
    let main_before = main_tip(&repo);

    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(!stage.merged);
    assert_eq!(main_tip(&repo), main_before);
}

#[test]
fn a_conflict_records_the_branch_head_once() {
    let (repo, mut orchestrator) = worktree_stage();
    make_conflict(&repo);
    let head = crate::git::runner::run_git_checked(&["rev-parse", "loom/s"], repo.path()).unwrap();

    orchestrator.record_merge_conflict(ID, 1);

    assert_eq!(on_disk(&orchestrator).completed_commit, Some(head.clone()));
    commit_file(&worktree(&repo), "later.txt", "resolver work");
    set_status(&orchestrator, StageStatus::MergeBlocked);
    orchestrator.record_merge_conflict(ID, 1);
    assert_eq!(on_disk(&orchestrator).completed_commit, Some(head));
}

#[test]
fn landing_a_control_path_branch_holds_it_for_human_review() {
    let (repo, mut orchestrator) = worktree_stage();
    std::fs::create_dir_all(worktree(&repo).join(".claude")).unwrap();
    commit_file(&worktree(&repo), ".claude/settings.json", "{}");
    let main_before = main_tip(&repo);

    assert_eq!(orchestrator.land_stage_merge(ID, "main"), Landing::Held);

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(!stage.merged);
    assert!(stage
        .review_reason
        .unwrap()
        .contains(".claude/settings.json"));
    assert_eq!(main_tip(&repo), main_before);
}

#[test]
fn a_landing_that_stashed_the_operators_changes_records_the_outcome() {
    let (repo, mut orchestrator) = worktree_stage();
    // The stage and the operator edit different lines of one tracked file, so
    // the merge touches a dirty path, needs a stash, and the reapply is clean.
    let lines: Vec<String> = (1..=20).map(|n| format!("line {n}\n")).collect();
    commit_file(repo.path(), "multi.txt", &lines.concat());
    git_ok(&worktree(&repo), &["reset", "-q", "--hard", "main"]);
    let stage_side = lines.concat().replace("line 20", "stage edit");
    commit_file(&worktree(&repo), "multi.txt", &stage_side);
    let operator_side = lines.concat().replace("line 1\n", "operator edit\n");
    std::fs::write(repo.path().join("multi.txt"), &operator_side).unwrap();

    assert!(orchestrator.try_auto_merge(ID));

    let stage = on_disk(&orchestrator);
    assert!(stage.merged);
    let stash = stage.merge.stash.expect("the stash outcome is recorded");
    assert!(stash.restored);
    assert!(stash.backup_ref.starts_with("refs/loom/autostash/"));
    git_ok(repo.path(), &["rev-parse", "--verify", &stash.backup_ref]);
    let merged = std::fs::read_to_string(repo.path().join("multi.txt")).unwrap();
    assert!(merged.starts_with("operator edit\n") && merged.contains("stage edit"));
}

#[test]
fn note_merge_stash_persists_an_unrestored_stash_and_ignores_none() {
    let (_repo, mut orchestrator) = worktree_stage();
    orchestrator.note_merge_stash(ID, None);
    assert_eq!(on_disk(&orchestrator).merge.stash, None);

    let lost = StashReapply {
        backup_ref: "refs/loom/autostash/s-1".to_string(),
        restored: false,
    };
    orchestrator.note_merge_stash(ID, Some(lost.clone()));
    assert_eq!(on_disk(&orchestrator).merge.stash, Some(lost));
}

#[test]
fn a_merge_signing_failure_holds_without_a_resolver() {
    let (repo, mut orchestrator) = worktree_stage();
    fake_signer(repo.path(), true);
    let main_before = main_tip(&repo);

    assert_eq!(orchestrator.land_stage_merge(ID, "main"), Landing::Held);

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    assert!(reason.contains("merge commit signing failed"), "{reason}");
    assert!(reason.contains("fake signer refused"), "{reason}");
    assert!(reason.contains("loom stage human-review s --approve"));
    assert_eq!(main_tip(&repo), main_before);
    assert_eq!(
        merge_resolver_attempts(&orchestrator.config.work_dir, ID),
        0
    );
}

#[path = "landing_signing_tests.rs"]
mod signing;
