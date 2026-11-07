//! The sweep's error path: a plan v2 stage whose completion cannot be
//! submitted is handed back to a fresh session with the error as its failure.

use super::*;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::StageType;
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
