//! The sweep's error path: a plan v2 stage whose completion cannot be
//! submitted is handed back to a fresh session with the error as its failure.
//! Then the operator's hold, the job that decides a held stage, and the
//! stage admission lock that `loom stage reset` holds.

use super::*;
use crate::fs::work_dir::write_terminal_config;
use crate::host_verify::apply::{apply_finished, Applied};
use crate::host_verify::store::load_current;
use crate::host_verify::submit::{cancel, submit, Submission, SubmitRequest};
use crate::host_verify::verdict_complete::{STAGE_BOUND_SESSION, VERDICT_REQUEST_ID};
use crate::host_verify::{JobKind, JobRecord, JobState};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::adjudication::feedback::read_feedback;
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{create_stage, load_stage, update_stage};

#[test]
fn a_submission_error_hands_the_stage_back_with_the_error_as_its_failure() {
    let temp = tempfile::TempDir::new().unwrap();
    let work = temp.path().join(".work");
    std::fs::create_dir_all(&work).unwrap();
    write_terminal_config(
        &work,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    let stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        stage_type: StageType::Standard,
        ..Stage::default()
    };
    create_stage(&stage, &work).unwrap();
    update_stage("test-stage", &work, |s| s.try_request_adjudication(None)).unwrap();
    // An unreadable job record makes the submission fail before any run exists.
    let jobs = work.join("verification").join("test-stage");
    std::fs::create_dir_all(&jobs).unwrap();
    std::fs::write(jobs.join("current.json"), "{ not json").unwrap();
    let config = OrchestratorConfig {
        work_dir: work.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut orchestrator =
        Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();

    orchestrator.start_verdict_completions();

    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
    let feedback = read_feedback(&work, "test-stage").unwrap().unwrap();
    assert!(
        feedback.contains("malformed verification/test-stage/current.json"),
        "the feedback must list the submission error: {feedback}"
    );
}

const STAGE_ID: &str = "test-stage";

/// A plan v2 stage a verdict left held (`NeedsAdjudication`, no session, no
/// open dispute), held by the operator too when `operator_hold`, with an
/// orchestrator over its state root.
fn verdict_held(operator_hold: bool) -> (tempfile::TempDir, std::path::PathBuf, Orchestrator) {
    let temp = tempfile::TempDir::new().unwrap();
    let work = temp.path().join(".work");
    std::fs::create_dir_all(&work).unwrap();
    let tmux = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(&work, &tmux).unwrap();
    let stage = Stage {
        id: STAGE_ID.to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        stage_type: StageType::Standard,
        ..Stage::default()
    };
    create_stage(&stage, &work).unwrap();
    update_stage(STAGE_ID, &work, |s| {
        s.try_request_adjudication(None)?;
        s.held = operator_hold;
        Ok(())
    })
    .unwrap();
    let config = OrchestratorConfig {
        work_dir: work.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let orchestrator =
        Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();
    (temp, work, orchestrator)
}

/// Queue the stage's VerdictComplete job as the sweep does.
fn queue_verdict_job(work: &std::path::Path) -> JobRecord {
    let request = SubmitRequest {
        stage_id: STAGE_ID,
        session_id: STAGE_BOUND_SESSION,
        request_id: VERDICT_REQUEST_ID,
        kind: JobKind::VerdictComplete,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: None,
    };
    match submit(work, &request, Utc::now()).unwrap() {
        Submission::Queued(record) => record,
        other => panic!("expected a queued job, got {other:?}"),
    }
}

#[test]
fn an_operator_held_stage_goes_back_to_the_queue_and_stays_held() {
    let (_temp, work, mut orchestrator) = verdict_held(true);

    orchestrator.start_verdict_completions();

    let stage = load_stage(STAGE_ID, &work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(stage.held, "the hand-back keeps the operator's hold");
    assert!(read_feedback(&work, STAGE_ID).unwrap().is_none());
}

#[test]
fn an_operator_held_stage_waits_for_the_job_that_decides_it() {
    let (_temp, work, mut orchestrator) = verdict_held(true);
    let job = queue_verdict_job(&work);

    orchestrator.start_verdict_completions();

    // The job's child may still run in the worktree: no fresh session yet.
    assert_eq!(
        load_stage(STAGE_ID, &work).unwrap().status,
        StageStatus::NeedsAdjudication
    );
    let current = load_current(&work, STAGE_ID).unwrap().unwrap();
    assert_eq!(
        (current.id.as_str(), current.state),
        (job.id.as_str(), JobState::Queued)
    );

    let cancelled = cancel(&work, STAGE_ID, "the job ended").unwrap().unwrap();
    let applied = apply_finished(&work, &cancelled).unwrap();
    assert!(matches!(applied, Applied::Ignored(_)), "{applied:?}");
    orchestrator.start_verdict_completions();

    let stage = load_stage(STAGE_ID, &work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(stage.held);
}

#[test]
fn a_stage_whose_admission_lock_is_held_waits_for_the_next_sweep() {
    let (_temp, work, mut orchestrator) = verdict_held(true);
    // `loom stage reset` holds this lock through its state write and cancel.
    let reset = crate::fs::stage_admission::lock(&work, STAGE_ID).unwrap();

    orchestrator.start_verdict_completions();

    assert_eq!(
        load_stage(STAGE_ID, &work).unwrap().status,
        StageStatus::NeedsAdjudication
    );
    drop(reset);
    orchestrator.start_verdict_completions();
    assert_eq!(
        load_stage(STAGE_ID, &work).unwrap().status,
        StageStatus::Queued
    );
}
