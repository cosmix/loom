//! Tests for the post-merge lifecycle.
//!
//! The property under test throughout: cleanup destroys a stage's worktree and
//! branch ONLY once git can prove the stage's work is in the target branch, and
//! an unanswerable git query counts as "not proven".

use super::*;
use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::branch_exists_strict;
use crate::git::merge::verify_merge_succeeded;
use crate::models::stage::Stage;
use crate::verify::transitions::create_stage;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Run git with ambient configuration neutralized, so a developer's global
/// settings (hooks, gpg signing, default branch, aliases) cannot change what
/// these tests exercise.
fn isolated_git(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

/// Assert a setup command succeeded. Dropping the exit status here would hide
/// setup failures and turn them into a confusing assertion further down.
fn git_ok(root: &Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository with one commit on `main` and a `.loom/work/` directory.
fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "test@example.com"]);
    git_ok(root, &["config", "user.name", "Loom Test"]);

    fs::write(root.join("file.txt"), "one\n").unwrap();
    git_ok(root, &["add", "file.txt"]);
    git_ok(root, &["commit", "-m", "initial"]);

    // Present so `WorkDir::new` resolves here instead of searching upward.
    fs::create_dir_all(root.join(".loom").join("work")).unwrap();

    temp
}

/// Add `.worktrees/<stage_id>` on a fresh `loom/<stage_id>` branch cut from
/// main, and commit one file on it. Returns the repo and that commit's SHA.
pub(super) fn repo_with_stage_commit(stage_id: &str) -> (TempDir, String) {
    let temp = init_repo();
    let root = temp.path();
    let path = worktree_of(root, stage_id);
    let branch = branch_name_for_stage(stage_id);
    git_ok(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            path.to_str().unwrap(),
            "main",
        ],
    );
    let head = commit_in_worktree(root, stage_id, "stage.txt", "stage work\n");
    (temp, head)
}

/// Commit one file inside the stage worktree. Returns the new HEAD SHA.
fn commit_in_worktree(root: &Path, stage_id: &str, name: &str, body: &str) -> String {
    let worktree = worktree_of(root, stage_id);
    fs::write(worktree.join(name), body).unwrap();
    git_ok(&worktree, &["add", name]);
    git_ok(&worktree, &["commit", "-m", "stage work"]);
    git_stdout(&worktree, &["rev-parse", "HEAD"])
}

pub(super) fn worktree_of(root: &Path, stage_id: &str) -> PathBuf {
    root.join(".worktrees").join(stage_id)
}

pub(super) fn merge_stage_branch(root: &Path, stage_id: &str) {
    let branch = branch_name_for_stage(stage_id);
    git_ok(root, &["merge", "--no-ff", "-m", "merge stage", &branch]);
}

pub(super) fn write_stage_record(work_dir: &Path, stage_id: &str, completed_commit: &str) {
    let stage = Stage {
        id: stage_id.to_string(),
        name: format!("Stage {stage_id}"),
        completed_commit: Some(completed_commit.to_string()),
        ..Stage::default()
    };
    create_stage(&stage, work_dir).unwrap();
}

pub(super) fn cleanup_from_outside(root: &Path, stage_id: &str) -> CleanupOutcome {
    // `root` is never inside the worktree, so this exercises the real path
    // without depending on the test runner's process-wide cwd.
    MergeLifecycle::new(stage_id, root, &root.join(".loom").join("work")).cleanup_with_cwd(
        Some(root),
        "main",
        &CleanupConfig::quiet(),
    )
}

fn refusal_reason(outcome: &CleanupOutcome) -> &str {
    match outcome {
        CleanupOutcome::Refused { reason } => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn assert_stage_survives(root: &Path, stage_id: &str) {
    assert!(
        worktree_of(root, stage_id).exists(),
        "a refused cleanup must leave the worktree in place"
    );
    assert!(
        branch_exists_strict(&branch_name_for_stage(stage_id), root).unwrap(),
        "a refused cleanup must leave the branch in place"
    );
}

#[test]
fn cleanup_refuses_while_the_stage_branch_holds_commits_the_target_lacks() {
    let stage_id = "unmerged-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    // Recorded, but main has never seen it: ancestry cannot be established.
    write_stage_record(&root.join(".loom").join("work"), stage_id, &head);

    let outcome = cleanup_from_outside(root, stage_id);

    let reason = refusal_reason(&outcome);
    assert!(reason.contains(stage_id), "must name the stage: {reason}");
    assert!(
        reason.contains(&branch_name_for_stage(stage_id)) && reason.contains("main"),
        "must name the branch and the target: {reason}"
    );
    assert_stage_survives(root, stage_id);
}

#[test]
fn cleanup_refuses_on_an_ahead_branch_even_with_no_stage_record_at_all() {
    // The stage-record lookup is not the guard; the branch check is. With no
    // record on disk the branch check must still refuse on its own.
    let stage_id = "recordless-stage";
    let (temp, _head) = repo_with_stage_commit(stage_id);
    let root = temp.path();

    let outcome = cleanup_from_outside(root, stage_id);

    let reason = refusal_reason(&outcome);
    assert!(
        reason.contains("still holds 1 commit"),
        "must refuse on the branch itself: {reason}"
    );
    assert_stage_survives(root, stage_id);
}

#[test]
fn cleanup_refuses_when_the_branch_advanced_past_a_verified_recorded_commit() {
    // The phantom-merge shape this module exists to prevent: `completed_commit`
    // is a pre-merge snapshot and IS in the target, but the branch moved on
    // afterwards (a resolver session, or the agent committing again). Verifying
    // the snapshot proves nothing about the branch that is about to be deleted.
    let stage_id = "advanced-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    write_stage_record(&root.join(".loom").join("work"), stage_id, &head);
    merge_stage_branch(root, stage_id);
    assert!(
        verify_merge_succeeded(&head, "main", root).unwrap(),
        "the recorded commit must be in main for this test to mean anything"
    );
    let later = commit_in_worktree(root, stage_id, "more.txt", "later work\n");
    assert!(!verify_merge_succeeded(&later, "main", root).unwrap());

    let outcome = cleanup_from_outside(root, stage_id);

    let reason = refusal_reason(&outcome);
    assert!(
        reason.contains("still holds 1 commit"),
        "a verified snapshot must not short-circuit the branch check: {reason}"
    );
    assert_stage_survives(root, stage_id);
}

#[test]
fn cleanup_refuses_when_the_branch_is_gone_but_the_worktree_holds_unmerged_work() {
    // Branch deleted, worktree left on a detached HEAD carrying commits main
    // never saw. Without the worktree-HEAD fallback this removes them silently.
    let stage_id = "detached-stage";
    let (temp, _head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let worktree = worktree_of(root, stage_id);
    git_ok(&worktree, &["checkout", "--detach"]);
    git_ok(root, &["branch", "-D", &branch_name_for_stage(stage_id)]);
    assert!(!branch_exists_strict(&branch_name_for_stage(stage_id), root).unwrap());

    let outcome = cleanup_from_outside(root, stage_id);

    let reason = refusal_reason(&outcome);
    assert!(
        reason.contains("worktree HEAD"),
        "must refuse on the worktree HEAD: {reason}"
    );
    assert!(worktree.exists(), "the worktree must survive a refusal");
}

#[test]
fn an_unanswerable_git_query_refuses_instead_of_proceeding() {
    // Not a git repository, so every git question fails. A swallowing existence
    // probe would read that as "no branch, nothing to lose" and proceed.
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let stage_id = "no-repo-stage";
    fs::create_dir_all(root.join(".loom").join("work")).unwrap();
    fs::create_dir_all(worktree_of(root, stage_id)).unwrap();

    let outcome = cleanup_from_outside(root, stage_id);

    let reason = refusal_reason(&outcome);
    assert!(
        reason.contains("cannot determine whether branch"),
        "an unanswerable existence probe must refuse: {reason}"
    );
    assert!(
        worktree_of(root, stage_id).exists(),
        "nothing may be removed on a refusal"
    );
}

#[test]
fn cleanup_proceeds_once_the_stage_branch_is_contained_by_the_target() {
    let stage_id = "merged-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    write_stage_record(&root.join(".loom").join("work"), stage_id, &head);
    merge_stage_branch(root, stage_id);

    let outcome = cleanup_from_outside(root, stage_id);

    match &outcome {
        CleanupOutcome::Done(result) => assert!(
            result.worktree_removed,
            "the worktree should have been removed: {result:?}"
        ),
        other => panic!("expected cleanup to run, got {other:?}"),
    }
    assert!(
        !worktree_of(root, stage_id).exists(),
        "the worktree directory must be gone"
    );
    assert!(!branch_exists_strict(&branch_name_for_stage(stage_id), root).unwrap());
}

#[test]
fn cleanup_reports_a_removal_failure_instead_of_propagating_it() {
    // Containment holds, but a stray non-scaffold entry makes worktree removal
    // refuse. The merge already landed, so this must not surface as an error.
    let stage_id = "unremovable-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    write_stage_record(&root.join(".loom").join("work"), stage_id, &head);
    merge_stage_branch(root, stage_id);
    let claude_dir = worktree_of(root, stage_id).join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("rogue.txt"), "not scaffold\n").unwrap();

    let outcome = cleanup_from_outside(root, stage_id);

    match &outcome {
        CleanupOutcome::Failed(reason) => {
            assert!(!reason.is_empty(), "a reported failure needs a reason")
        }
        other => panic!("expected a reported failure, got {other:?}"),
    }
    assert!(
        worktree_of(root, stage_id).exists(),
        "a failed cleanup leaves the worktree behind"
    );
}

#[test]
fn cleanup_defers_while_the_cwd_is_inside_the_worktree_it_would_remove() {
    let stage_id = "live-stage";
    let (temp, _head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let worktree = worktree_of(root, stage_id);
    let work_dir = root.join(".loom").join("work");
    let lifecycle = MergeLifecycle::new(stage_id, root, &work_dir);
    let quiet = CleanupConfig::quiet();

    // Injected rather than set via `set_current_dir`: mutating the process cwd
    // is a global side effect that breaks tests running in parallel.
    let inside = lifecycle.cleanup_with_cwd(Some(&worktree), "main", &quiet);
    assert!(
        matches!(inside, CleanupOutcome::Deferred),
        "a cwd inside the worktree must defer, got {inside:?}"
    );

    let unknown = lifecycle.cleanup_with_cwd(None, "main", &quiet);
    assert!(
        matches!(unknown, CleanupOutcome::Deferred),
        "an undeterminable cwd must fail closed and defer, got {unknown:?}"
    );

    assert!(worktree.exists(), "a deferred cleanup removes nothing");
}

#[test]
fn should_defer_cleanup_answers_on_containment_of_the_worktree_path() {
    // Pure path predicate: no repository needed, only directories on disk.
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let stage_id = "path-stage";
    let worktree = worktree_of(root, stage_id);
    let nested = worktree.join("nested");
    fs::create_dir_all(&nested).unwrap();

    assert!(should_defer_cleanup(&worktree, root, stage_id));
    assert!(should_defer_cleanup(&nested, root, stage_id));
    assert!(!should_defer_cleanup(root, root, stage_id));
    assert!(
        !should_defer_cleanup(root, root, "never-existed"),
        "an absent worktree makes cleanup a no-op, not a deferral"
    );
    assert!(
        should_defer_cleanup(&worktree.join("does-not-exist"), root, stage_id),
        "an uncanonicalizable cwd must fail closed and defer"
    );
}

#[test]
fn a_stage_with_no_worktree_and_no_branch_is_nothing_to_do() {
    let temp = init_repo();
    let root = temp.path();

    let outcome = cleanup_from_outside(root, "never-existed");

    assert!(
        matches!(outcome, CleanupOutcome::NothingToDo),
        "got {outcome:?}"
    );
}

#[test]
fn the_post_merge_tail_still_cleans_up_when_the_base_reconcile_cannot_succeed() {
    let stage_id = "tail-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    write_stage_record(&work_dir, stage_id, &head);
    merge_stage_branch(root, stage_id);

    // No plan config and no populated context cache, so the source-graph
    // reconcile has nothing to work with. A degraded semantic layer must never
    // block or undo a merge that git has already accepted.
    let outcome = finish_verified_merge(stage_id, root, &work_dir, "main", &CleanupConfig::quiet());

    assert!(
        matches!(outcome, CleanupOutcome::Done(_)),
        "the tail must still clean up, got {outcome:?}"
    );
    assert!(!worktree_of(root, stage_id).exists());
}
