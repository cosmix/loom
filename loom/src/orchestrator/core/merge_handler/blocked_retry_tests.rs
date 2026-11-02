//! The per-tick retry of a blocked merge skips an attempt whose inputs did
//! not change, because each attempt writes git objects into the main checkout.

use std::path::Path;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::super::landing::Landing;
use super::super::resolver_spawn::test_fixtures::{
    git_ok, orchestrator_with_conflict, repo_with_stage_branches,
};
use super::watches_directory;
use crate::git::MergeBlock;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::Orchestrator;
use crate::verify::transitions::{load_stage, update_stage};

const ID: &str = "s";

#[path = "blocked_retry_backup_ref_tests.rs"]
mod backup_ref;

/// A stage `s` with a worktree on `loom/s` holding one commit, an
/// orchestrator over it, and an untracked `stage.txt` in the main checkout
/// that blocks the merge. The state directories are excluded from the
/// checkout's status, as they are in a real project.
pub(super) fn blocked_stage() -> (TempDir, Orchestrator) {
    let repo = repo_with_stage_branches(&[]);
    let root = repo.path();
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    let worktree = root.join(".worktrees").join(ID);
    git_ok(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "loom/s",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("stage.txt"), "stage").unwrap();
    git_ok(&worktree, &["add", "stage.txt"]);
    git_ok(&worktree, &["commit", "-q", "-m", "stage"]);

    let mut orchestrator = orchestrator_with_conflict(root, ID);
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.status = StageStatus::Completed;
        stage.completed_commit = Some(crate::git::get_branch_head("loom/s", root)?);
        Ok(())
    })
    .unwrap();
    std::fs::write(root.join("stage.txt"), "operator's own file").unwrap();
    assert!(!orchestrator.try_auto_merge(ID));
    (repo, orchestrator)
}

pub(super) fn on_disk(orchestrator: &Orchestrator) -> Stage {
    load_stage(ID, &orchestrator.config.work_dir).unwrap()
}

fn stage_file_text(orchestrator: &Orchestrator) -> String {
    let stages = orchestrator.config.work_dir.join("stages");
    let file = crate::fs::stage_files::find_stage_file(&stages, ID)
        .unwrap()
        .unwrap();
    std::fs::read_to_string(file).unwrap()
}

/// The `count` and `in-pack` lines of `git count-objects -v`.
fn object_counts(root: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["count-objects", "-v"])
        .current_dir(root)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.starts_with("count:") || line.starts_with("in-pack:"))
        .map(String::from)
        .collect()
}

#[test]
fn an_unchanged_block_is_not_retried_and_writes_no_object() {
    let (repo, mut orchestrator) = blocked_stage();

    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    let objects = object_counts(repo.path());
    let stage_file = stage_file_text(&orchestrator);
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    assert_eq!(object_counts(repo.path()), objects);
    assert_eq!(stage_file_text(&orchestrator), stage_file);
    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);
    assert!(orchestrator.blocked_merge_inputs.contains_key(ID));
}

#[test]
fn a_cleared_block_is_retried_and_lands() {
    let (repo, mut orchestrator) = blocked_stage();
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    git_ok(repo.path(), &["stash", "push", "-q", "--include-untracked"]);
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert!(!orchestrator.blocked_merge_inputs.contains_key(ID));
}

#[test]
fn deleting_an_ignored_file_named_by_the_block_clears_the_skip() {
    let (repo, mut orchestrator) = blocked_stage();
    let exclude = repo.path().join(".git/info/exclude");
    let text = std::fs::read_to_string(&exclude).unwrap();
    std::fs::write(&exclude, format!("{text}stage.txt\n")).unwrap();
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);
    assert!(orchestrator.blocked_merge_inputs.contains_key(ID));

    std::fs::remove_file(repo.path().join("stage.txt")).unwrap();
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
}

fn minutes(count: u64) -> Duration {
    Duration::from_secs(count * 60)
}

fn refused() -> Landing {
    Landing::Blocked(MergeBlock::FastForwardRefused {
        detail: "index.lock exists".to_string(),
    })
}

#[test]
fn a_fast_forward_refusal_is_timed_and_never_memoized() {
    let (_repo, mut orchestrator) = blocked_stage();
    let now = Instant::now();

    orchestrator.remember_blocked_inputs(ID, Some(7), &refused(), now);

    assert!(!orchestrator.blocked_merge_inputs.contains_key(ID));
    assert_eq!(orchestrator.refused_merge_attempts.get(ID), Some(&now));
    assert!(orchestrator.refused_retry_pending(ID, now + Duration::from_secs(59)));
    assert!(!orchestrator.refused_retry_pending(ID, now + minutes(1)));
}

#[test]
fn a_refused_merge_is_not_retried_within_a_minute_and_is_after() {
    let (repo, mut orchestrator) = blocked_stage();
    let now = Instant::now();
    git_ok(repo.path(), &["stash", "push", "-q", "--include-untracked"]);
    orchestrator
        .refused_merge_attempts
        .insert(ID.to_string(), now);

    orchestrator.retry_blocked_merge_at(&on_disk(&orchestrator), now + Duration::from_secs(30));
    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);

    orchestrator.retry_blocked_merge_at(&on_disk(&orchestrator), now + minutes(2));
    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert!(!orchestrator.refused_merge_attempts.contains_key(ID));
}

#[test]
fn a_memo_entry_older_than_ten_minutes_is_retried() {
    let (_repo, mut orchestrator) = blocked_stage();
    let start = Instant::now();
    orchestrator.retry_blocked_merge_at(&on_disk(&orchestrator), start);
    let (inputs, at) = orchestrator.blocked_merge_inputs[ID];
    assert_eq!(at, start);

    orchestrator.retry_blocked_merge_at(&on_disk(&orchestrator), start + minutes(9));
    assert_eq!(orchestrator.blocked_merge_inputs[ID], (inputs, start));

    let later = start + minutes(11);
    orchestrator.retry_blocked_merge_at(&on_disk(&orchestrator), later);
    assert_eq!(orchestrator.blocked_merge_inputs[ID], (inputs, later));
}

#[test]
fn a_watched_directory_disables_memoization() {
    let (repo, mut orchestrator) = blocked_stage();
    std::fs::create_dir(repo.path().join("ignored_dir")).unwrap();
    let mut stage = on_disk(&orchestrator);
    stage.merge.block = Some(MergeBlock::UncommittedOverlap {
        paths: vec!["stage.txt".to_string(), "ignored_dir".to_string()],
    });

    assert!(watches_directory(repo.path(), &["ignored_dir".to_string()]));
    assert!(!watches_directory(repo.path(), &["stage.txt".to_string()]));
    orchestrator.retry_blocked_merge_at(&stage, Instant::now());

    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);
    assert!(!orchestrator.blocked_merge_inputs.contains_key(ID));
}

#[test]
fn a_stage_that_left_merge_blocked_loses_its_retry_memo() {
    let (_repo, mut orchestrator) = blocked_stage();
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    orchestrator
        .refused_merge_attempts
        .insert(ID.to_string(), Instant::now());

    orchestrator.prune_retry_memos();
    assert!(orchestrator.blocked_merge_inputs.contains_key(ID));
    assert!(orchestrator.refused_merge_attempts.contains_key(ID));

    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.route_to_review("operator takes over");
        Ok(())
    })
    .unwrap();
    orchestrator.prune_retry_memos();

    assert!(!orchestrator.blocked_merge_inputs.contains_key(ID));
    assert!(!orchestrator.refused_merge_attempts.contains_key(ID));
}

#[test]
fn a_memo_survives_a_stage_file_that_cannot_be_read() {
    let (_repo, mut orchestrator) = blocked_stage();
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    let stages = orchestrator.config.work_dir.join("stages");
    let file = crate::fs::stage_files::find_stage_file(&stages, ID)
        .unwrap()
        .unwrap();
    std::fs::write(file, "---\n: not yaml [\n---\n").unwrap();
    assert!(load_stage(ID, &orchestrator.config.work_dir).is_err());

    orchestrator.prune_retry_memos();

    assert!(orchestrator.blocked_merge_inputs.contains_key(ID));
}

/// A git error while the block's retry runs says nothing about the block: it
/// stays, and with it the promise that no resolver spawns for a merge with no
/// conflict. The error is recorded beside it, so the stage says what the retry
/// hits. The next retry lands once git answers again.
#[test]
fn an_infrastructure_error_in_a_retry_keeps_the_typed_block() {
    let (repo, mut orchestrator) = blocked_stage();
    let before = on_disk(&orchestrator);
    let block = before.merge.block.clone().expect("the fixture is blocked");
    let tip = crate::git::get_branch_head("main", repo.path()).unwrap();
    git_ok(repo.path(), &["update-ref", "-d", "refs/heads/main"]);

    orchestrator.retry_blocked_merge(&before);

    let failed = on_disk(&orchestrator);
    assert_eq!(failed.status, StageStatus::MergeBlocked);
    assert_eq!(failed.merge.block.as_ref(), Some(&block));
    assert_ne!(failed.failure_info, before.failure_info);

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert!(!orchestrator.active_sessions.contains_key(ID));
    assert_eq!(
        super::super::resolver_attempts::merge_resolver_attempts(&orchestrator.config.work_dir, ID),
        0
    );

    git_ok(repo.path(), &["update-ref", "refs/heads/main", &tip]);
    git_ok(repo.path(), &["stash", "push", "-q", "--include-untracked"]);
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    assert!(on_disk(&orchestrator).merged);
}

#[test]
fn persisting_a_different_block_archives_the_first_failure_once_and_an_identical_one_nothing() {
    let (_repo, mut orchestrator) = blocked_stage();
    let mut stage = on_disk(&orchestrator);
    let first = stage.failure_info.clone().expect("the block recorded one");

    orchestrator.persist_merge_blocked(&mut stage, ID, "a different error");
    let second = on_disk(&orchestrator);
    assert_eq!(second.issue_history.len(), 1);
    assert_eq!(second.issue_history[0].evidence, first.evidence);
    let recorded = second.failure_info.clone().expect("the new failure stands");
    assert_eq!(recorded.evidence, ["a different error"]);

    orchestrator.persist_merge_blocked(&mut stage, ID, "a different error");
    let third = on_disk(&orchestrator);
    assert_eq!(third.issue_history.len(), 1);
    let kept = third.failure_info.expect("still recorded");
    assert_eq!(kept.detected_at, recorded.detected_at);
}

#[test]
fn a_stage_that_became_final_on_disk_is_not_moved_to_merge_blocked() {
    let (_repo, mut orchestrator) = blocked_stage();
    let mut stale = on_disk(&orchestrator);
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.status = StageStatus::Completed;
        stage.merged = true;
        Ok(())
    })
    .unwrap();

    orchestrator.persist_merge_blocked(&mut stale, ID, "late failure");

    let fresh = on_disk(&orchestrator);
    assert_eq!(fresh.status, StageStatus::Completed);
    assert!(fresh.merged);
}
