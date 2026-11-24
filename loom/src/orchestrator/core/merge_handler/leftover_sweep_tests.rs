//! The leftover sweep: finish deferred cleanups of merged stages, once.

use std::path::Path;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::super::resolver_spawn::test_fixtures::{
    commit_on_stage_branch, repo_with_stage_branches,
};
use crate::models::session::SessionType;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::merge_lifecycle::test_support::{finish_session, write_live_session};
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::test_git::git;
use crate::verify::transitions::{load_stage, save_stage, update_stage};

const ID: &str = "swept";

fn head(root: &Path, rev: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(root)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn worktree(root: &Path) -> std::path::PathBuf {
    root.join(".worktrees").join(ID)
}

fn branch_exists(root: &Path) -> bool {
    std::process::Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/loom/{ID}"),
        ])
        .current_dir(root)
        .output()
        .unwrap()
        .status
        .success()
}

/// A repo with branch `loom/swept` (one commit ahead of `main` when `ahead`)
/// checked out in `.worktrees/swept`, and an orchestrator whose graph and
/// stage file hold `swept` as `Completed` and `merged` with `main`'s head as
/// its recorded commit.
fn merged_stage(stage_type: StageType, ahead: bool) -> (TempDir, Orchestrator) {
    let repo = repo_with_stage_branches(&[ID]);
    let root = repo.path();
    if ahead {
        commit_on_stage_branch(root, ID, "work.txt");
    }
    git(
        root,
        &[
            "worktree",
            "add",
            worktree(root).to_str().unwrap(),
            &format!("loom/{ID}"),
        ],
    );
    let work_dir = root.join(".loom").join("work");
    let stage = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        merged: true,
        stage_type,
        completed_commit: Some(head(root, "main")),
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();

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
    let config = OrchestratorConfig {
        work_dir,
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    (repo, Orchestrator::new(config, graph).unwrap())
}

#[test]
fn a_merged_stage_is_cleaned_up_and_not_swept_again() {
    let (repo, mut orchestrator) = merged_stage(StageType::Standard, false);
    let root = repo.path();

    orchestrator.sweep_merged_leftovers();

    assert!(!worktree(root).exists());
    assert!(!branch_exists(root));
    assert!(orchestrator.settled_leftovers.contains(ID));

    // A settled stage costs no git work: a recreated directory stays.
    std::fs::create_dir_all(worktree(root)).unwrap();
    orchestrator.sweep_merged_leftovers();
    assert!(worktree(root).exists());
}

#[test]
fn a_live_session_defers_the_sweep_until_it_ends() {
    let (repo, mut orchestrator) = merged_stage(StageType::Standard, false);
    let root = repo.path();
    let work_dir = orchestrator.config.work_dir.clone();
    let mut session = write_live_session(&work_dir, ID, SessionType::Stage);

    orchestrator.sweep_merged_leftovers();
    assert!(worktree(root).exists(), "a live session keeps the worktree");
    assert!(!orchestrator.settled_leftovers.contains(ID));
    assert!(load_stage(ID, &work_dir).unwrap().cleanup_warning.is_none());

    finish_session(&work_dir, &mut session);
    orchestrator.sweep_merged_leftovers();
    assert!(!worktree(root).exists());
    assert!(orchestrator.settled_leftovers.contains(ID));
}

#[test]
fn a_branch_holding_unmerged_commits_is_refused_once() {
    let (repo, mut orchestrator) = merged_stage(StageType::Standard, true);
    let root = repo.path();
    let work_dir = orchestrator.config.work_dir.clone();

    orchestrator.sweep_merged_leftovers();

    assert!(worktree(root).exists());
    assert!(branch_exists(root));
    let warning = load_stage(ID, &work_dir).unwrap().cleanup_warning;
    assert!(
        warning.is_some_and(|warning| warning.starts_with("refused")),
        "the refusal is recorded on the stage"
    );
    assert!(orchestrator.settled_leftovers.contains(ID), "not retried");
}

#[test]
fn a_knowledge_stage_is_left_untouched() {
    let (repo, mut orchestrator) = merged_stage(StageType::Knowledge, false);
    let root = repo.path();

    orchestrator.sweep_merged_leftovers();

    assert!(worktree(root).exists());
    assert!(branch_exists(root));
}

#[test]
fn a_deferral_over_ten_minutes_records_one_warning_and_keeps_retrying() {
    let (repo, mut orchestrator) = merged_stage(StageType::Standard, false);
    let root = repo.path();
    let work_dir = orchestrator.config.work_dir.clone();
    let mut session = write_live_session(&work_dir, ID, SessionType::Stage);
    let start = Instant::now();

    orchestrator.sweep_merged_leftovers_at(start);
    orchestrator.sweep_merged_leftovers_at(start + Duration::from_secs(9 * 60));
    assert!(load_stage(ID, &work_dir).unwrap().cleanup_warning.is_none());

    orchestrator.sweep_merged_leftovers_at(start + Duration::from_secs(11 * 60));
    let warning = load_stage(ID, &work_dir).unwrap().cleanup_warning;
    let text = warning.expect("the long deferral is recorded");
    assert!(text.contains("deferred for over 10 minutes"), "{text}");
    assert!(
        text.contains(&format!("loom worktree remove {ID}")),
        "{text}"
    );
    assert!(!orchestrator.settled_leftovers.contains(ID));

    // The warning is written once: a later edit of it is not overwritten.
    update_stage(ID, &work_dir, |stage| {
        stage.cleanup_warning = Some("edited".to_string());
        Ok(())
    })
    .unwrap();
    orchestrator.sweep_merged_leftovers_at(start + Duration::from_secs(12 * 60));
    assert_eq!(
        load_stage(ID, &work_dir)
            .unwrap()
            .cleanup_warning
            .as_deref(),
        Some("edited")
    );

    finish_session(&work_dir, &mut session);
    orchestrator.sweep_merged_leftovers_at(start + Duration::from_secs(13 * 60));
    assert!(!worktree(root).exists());
    assert!(orchestrator.settled_leftovers.contains(ID));
    assert!(!orchestrator.deferred_cleanups.contains_key(ID));
    assert!(load_stage(ID, &work_dir).unwrap().cleanup_warning.is_none());
}
