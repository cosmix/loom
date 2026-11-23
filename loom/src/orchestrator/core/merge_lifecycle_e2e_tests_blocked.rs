//! The block and clean-landing half of the merge lifecycle, end to end with
//! real git: a typed block retried each tick, a target not checked out in the
//! operator's checkout, an operator edit on a path the merge touches, deferred
//! cleanup, and a daemon restart. After every step the operator's checkout is
//! checked against a snapshot.

use super::merge_lifecycle_e2e_tests_support::*;
use crate::git::MergeBlock;
use crate::models::session::SessionType;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::merge_resolver_attempts;
use crate::orchestrator::merge_lifecycle::test_support::{finish_session, write_live_session};
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::save_stage;

/// `multi.txt` committed on `main`, then the stage rewrites line 10 in its
/// worktree and the operator edits line 9 (adjacent: the stage's change cannot
/// be reapplied over it) without committing.
fn blocked_by_adjacent_edit() -> tempfile::TempDir {
    let repo = repo_with_worktree();
    let root = repo.path();
    commit_file(root, "multi.txt", &twenty_lines());
    git(&worktree(root), &["reset", "-q", "--hard", "main"]);
    let stage_side = twenty_lines().replace("line 10\n", "stage edit\n");
    commit_file(&worktree(root), "multi.txt", &stage_side);
    let operator_side = twenty_lines().replace("line 9\n", "operator edit\n");
    std::fs::write(root.join("multi.txt"), operator_side).unwrap();
    repo
}

#[test]
fn a_typed_block_is_retried_quietly_and_lands_once_the_operator_clears_it() {
    let repo = blocked_by_adjacent_edit();
    let root = repo.path();
    let before = checkout(root);
    let main_before = main_tip(root);
    let mut orchestrator = orchestrator_with_completed_stage(root);
    let work_dir = orchestrator.config.work_dir.clone();

    assert!(!orchestrator.try_auto_merge(ID));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    assert_eq!(
        stage.merge.block,
        Some(MergeBlock::UncommittedOverlap {
            paths: vec!["multi.txt".to_string()]
        })
    );
    assert_eq!(merge_resolver_attempts(&work_dir, ID), 0);
    assert_untouched(root, &before);

    let first = orchestrator.spawn_merge_resolution_sessions();
    assert_eq!(first.unwrap(), 0);
    assert_untouched(root, &before);
    let commits = commit_objects(root);
    let stage_file = stage_file_text(&orchestrator);

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);
    assert_eq!(commit_objects(root), commits, "a retry wrote a commit");
    assert_eq!(stage_file_text(&orchestrator), stage_file);
    assert_eq!(main_tip(root), main_before);
    assert_untouched(root, &before);

    git(root, &["stash", "push", "-q"]);
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(stage.merge.block, None);
    assert!(stage.failure_info.is_none());
    assert!(!worktree(root).exists(), "no live session holds it");
    assert!(!crate::git::merge::merge_head_exists(root).unwrap());
    assert_eq!(
        text_of(root, "multi.txt").lines().nth(9),
        Some("stage edit")
    );
}

#[test]
fn a_target_not_checked_out_in_the_operators_checkout_advances_by_ref_alone() {
    let repo = repo_with_worktree();
    let root = repo.path();
    commit_file(&worktree(root), "work.txt", "stage work");
    git(root, &["checkout", "-q", "-b", "other"]);
    std::fs::write(root.join("seed.txt"), "operator edit").unwrap();
    std::fs::write(root.join("scratch.txt"), "untracked\n").unwrap();
    let before = checkout(root);
    let main_before = main_tip(root);
    let mut orchestrator = orchestrator_with_completed_stage(root);

    assert!(orchestrator.try_auto_merge(ID));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_ne!(main_tip(root), main_before);
    assert_eq!(git(root, &["show", "main:work.txt"]), "stage work");
    assert_untouched(root, &before);
    assert_eq!(git(root, &["symbolic-ref", "HEAD"]), "refs/heads/other");
}

#[test]
fn an_operator_edit_on_a_path_the_merge_touches_blocks_the_landing() {
    let repo = repo_with_worktree();
    let root = repo.path();
    commit_file(root, "multi.txt", &twenty_lines());
    git(&worktree(root), &["reset", "-q", "--hard", "main"]);
    let stage_side = twenty_lines().replace("line 20\n", "stage edit\n");
    commit_file(&worktree(root), "multi.txt", &stage_side);
    let operator_side = twenty_lines().replace("line 1\n", "operator edit\n");
    std::fs::write(root.join("multi.txt"), operator_side).unwrap();
    let main_before = main_tip(root);
    let mut orchestrator = orchestrator_with_completed_stage(root);

    assert!(!orchestrator.try_auto_merge(ID));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    assert!(
        matches!(
            &stage.merge.block,
            Some(MergeBlock::UncommittedOverlap { paths }) if paths == &["multi.txt"]
        ),
        "{:?}",
        stage.merge.block
    );
    assert!(text_of(root, "multi.txt").starts_with("operator edit\n"));
    assert_eq!(main_tip(root), main_before);
}

#[test]
fn a_live_session_defers_cleanup_and_the_sweep_finishes_it_after_the_session_ends() {
    let repo = repo_with_worktree();
    let root = repo.path();
    let graph = merged_graph();
    let mut orchestrator = orchestrator_with_graph(root, graph);
    let merged = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        merged: true,
        completed_commit: Some(main_tip(root)),
        ..Stage::default()
    };
    save_stage(&merged, &orchestrator.config.work_dir).unwrap();
    let work_dir = orchestrator.config.work_dir.clone();
    let mut session = write_live_session(&work_dir, ID, SessionType::Stage);
    let before = checkout(root);

    orchestrator.sweep_merged_leftovers();

    assert!(worktree(root).is_dir(), "a live session holds it");
    assert_untouched(root, &before);

    finish_session(&work_dir, &mut session);
    orchestrator.sweep_merged_leftovers();

    assert!(!worktree(root).exists());
    assert_untouched(root, &before);
}

/// A graph holding stage `s` as completed and merged.
fn merged_graph() -> ExecutionGraph {
    let mut graph = ExecutionGraph::build(vec![StageDefinition {
        id: ID.to_string(),
        name: ID.to_string(),
        working_dir: ".".to_string(),
        ..Default::default()
    }])
    .unwrap();
    graph.mark_queued(ID).unwrap();
    graph.mark_executing(ID).unwrap();
    graph.mark_completed(ID).unwrap();
    graph.mark_merged(ID).unwrap();
    graph
}

#[test]
fn a_restarted_daemon_lands_a_persisted_block_once_its_cause_is_gone() {
    let repo = blocked_by_adjacent_edit();
    let root = repo.path();
    {
        let mut first = orchestrator_with_completed_stage(root);
        assert!(!first.try_auto_merge(ID));
        assert_eq!(on_disk(&first).status, StageStatus::MergeBlocked);
    }
    git(root, &["stash", "push", "-q"]);
    let mut restarted = orchestrator(root);
    assert_eq!(on_disk(&restarted).status, StageStatus::MergeBlocked);
    assert!(on_disk(&restarted).merge.block.is_some());

    assert_eq!(restarted.spawn_merge_resolution_sessions().unwrap(), 0);

    let stage = on_disk(&restarted);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(stage.merge.block, None);
    assert!(!worktree(root).exists());
    assert!(!crate::git::merge::merge_head_exists(root).unwrap());
}
