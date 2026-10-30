//! Handing a committed merge back to a stage session: what it stops, what the
//! stage file and graph read afterwards, and when it holds for the operator.

use super::super::resolver_attempts::attempts_file;
use super::super::resolver_spawn::test_fixtures::{
    orchestrator_with_conflict, repo_with_stage_branches, tracked_resolver, with_read_only_stages,
    write_pid_file,
};
use crate::fs::session_files::load_session_exact;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionBackendKind, SessionExitReason, SessionStatus};
use crate::models::stage::StageStatus;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, update_stage};

/// An orchestrator whose graph holds `stage_id` as `MergeConflict`, over a
/// stage file that records a completed commit.
fn orchestrator_with_node(repo: &std::path::Path, stage_id: &str) -> Orchestrator {
    let mut orchestrator = orchestrator_with_conflict(repo, stage_id);
    orchestrator.graph = ExecutionGraph::build(vec![StageDefinition {
        id: stage_id.to_string(),
        name: stage_id.to_string(),
        working_dir: ".".to_string(),
        ..Default::default()
    }])
    .unwrap();
    orchestrator
        .graph
        .force_status(stage_id, StageStatus::MergeConflict)
        .unwrap();
    update_stage(stage_id, &orchestrator.config.work_dir, |stage| {
        stage.completed_commit = Some("abc123".to_string());
        Ok(())
    })
    .unwrap();
    orchestrator
}

#[test]
fn a_failed_verification_stops_the_resolver_and_requeues_the_stage() {
    let repo = repo_with_stage_branches(&["handed"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "handed");
    let work_dir = orchestrator.config.work_dir.clone();
    let (resolver, signal) = tracked_resolver(&mut orchestrator, "handed");

    let note = orchestrator
        .hand_merge_back("handed", "Acceptance criteria failed: tests")
        .unwrap();

    assert!(note.contains("handed back"), "{note}");
    let stage = load_stage("handed", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(!stage.merge_conflict);
    assert_eq!(stage.merge.handbacks, 1);
    assert_eq!(stage.session, None);
    assert_eq!(
        stage.merge.handback_refusal.as_deref(),
        Some("Acceptance criteria failed: tests")
    );
    assert_eq!(stage.completed_commit.as_deref(), Some("abc123"));
    assert!(!stage.merged);
    assert_eq!(
        orchestrator.graph.get_node("handed").unwrap().status,
        StageStatus::Queued
    );
    assert!(!orchestrator.active_sessions.contains_key("handed"));
    assert!(!signal.exists());
    assert!(!attempts_file(&work_dir, "handed").exists());
    let record = load_session_exact(&work_dir, &resolver.id)
        .unwrap()
        .unwrap();
    assert_eq!(record.exit_reason, Some(SessionExitReason::OperatorStop));
}

#[test]
fn a_stop_that_could_not_be_proven_holds_the_stage_for_the_operator() {
    let repo = repo_with_stage_branches(&["unproven"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "unproven");
    let work_dir = orchestrator.config.work_dir.clone();
    let (resolver, _) = tracked_resolver(&mut orchestrator, "unproven");
    write_pid_file(&work_dir, &resolver, None);

    orchestrator
        .hand_merge_back("unproven", "tests failed")
        .unwrap();

    let stage = load_stage("unproven", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(stage
        .review_reason
        .unwrap()
        .contains("may still be running"));
    assert_eq!(stage.merge.handbacks, 0);
    assert_eq!(
        stage.merge.handback_refusal.as_deref(),
        Some("tests failed")
    );
    assert!(orchestrator.active_sessions.contains_key("unproven"));
}

#[test]
fn a_stage_that_used_its_handbacks_goes_to_the_operator_with_the_failure() {
    let repo = repo_with_stage_branches(&["capped"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "capped");
    let work_dir = orchestrator.config.work_dir.clone();
    update_stage("capped", &work_dir, |stage| {
        stage.merge.handbacks = 2;
        Ok(())
    })
    .unwrap();

    orchestrator
        .hand_merge_back("capped", "tests failed")
        .unwrap();

    let stage = load_stage("capped", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    assert!(reason.contains("--approve"), "{reason}");
    assert!(reason.contains("tests failed"), "{reason}");
    assert_eq!(
        stage.merge.handback_refusal.as_deref(),
        Some("tests failed")
    );
}

#[test]
fn a_blocked_merge_without_a_typed_block_is_handed_back() {
    let repo = repo_with_stage_branches(&["blocked"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "blocked");
    let work_dir = orchestrator.config.work_dir.clone();
    update_stage("blocked", &work_dir, |stage| {
        stage.status = StageStatus::MergeBlocked;
        Ok(())
    })
    .unwrap();

    orchestrator
        .hand_merge_back("blocked", "tests failed")
        .unwrap();

    assert_eq!(
        load_stage("blocked", &work_dir).unwrap().status,
        StageStatus::Queued
    );
}

#[test]
fn a_stage_that_left_its_merge_state_is_not_handed_back() {
    let repo = repo_with_stage_branches(&["moved"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "moved");
    let work_dir = orchestrator.config.work_dir.clone();
    update_stage("moved", &work_dir, |stage| {
        stage.status = StageStatus::Executing;
        Ok(())
    })
    .unwrap();

    let refusal = orchestrator
        .hand_merge_back("moved", "tests failed")
        .unwrap_err();

    assert!(refusal.contains("left its merge state"), "{refusal}");
    let stage = load_stage("moved", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.merge.handbacks, 0);
}

#[test]
fn the_last_handback_below_the_cap_still_requeues() {
    let repo = repo_with_stage_branches(&["last"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "last");
    let work_dir = orchestrator.config.work_dir.clone();
    update_stage("last", &work_dir, |stage| {
        stage.merge.handbacks = 1;
        Ok(())
    })
    .unwrap();

    orchestrator
        .hand_merge_back("last", "tests failed")
        .unwrap();

    let stage = load_stage("last", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.merge.handbacks, 2);
}

#[test]
fn a_live_worker_session_holds_the_stage_for_the_operator() {
    let repo = repo_with_stage_branches(&["busy"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "busy");
    let work_dir = orchestrator.config.work_dir.clone();
    let mut worker = Session::new();
    worker.assign_to_stage("busy".to_string());
    worker.status = SessionStatus::Running;
    worker.backend = SessionBackendKind::Tmux;
    save_session(&worker, &work_dir).unwrap();
    write_test_pid_identity(&work_dir, &worker, std::process::id()).unwrap();

    orchestrator
        .hand_merge_back("busy", "tests failed")
        .unwrap();

    let stage = load_stage("busy", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(stage.merge.handbacks, 0);
}

#[test]
fn a_requeue_that_cannot_be_saved_is_an_error_and_changes_nothing() {
    let repo = repo_with_stage_branches(&["unsaved"]);
    let mut orchestrator = orchestrator_with_node(repo.path(), "unsaved");
    let work_dir = orchestrator.config.work_dir.clone();

    let Some(result) = with_read_only_stages(&mut orchestrator, |orchestrator| {
        orchestrator.hand_merge_back("unsaved", "tests failed")
    }) else {
        return;
    };

    assert!(result.unwrap_err().contains("could not be saved"));
    let stage = load_stage("unsaved", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(stage.merge.handbacks, 0);
}
