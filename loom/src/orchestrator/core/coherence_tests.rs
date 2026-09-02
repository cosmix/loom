//! Unit tests for [`super::Orchestrator::reconcile_executing_stages`], the
//! per-tick watchdog for an `Executing` stage that no longer names a live
//! worker session of its own kind.
//!
//! Fixtures mirror `stage_executor_tests.rs`: they are private to that
//! module, so the same small set is duplicated here.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::failure::FailureType;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::Stage;
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage};
use tempfile::TempDir;

/// A `.work` directory whose configured terminal lane is tmux, so
/// `Orchestrator::new` never runs real terminal detection.
fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".work");
    std::fs::create_dir_all(&work).unwrap();
    write_terminal_config(
        &work,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    temp
}

fn orchestrator_for(work_dir: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap()
}

fn stage_at(work_dir: &Path, stage_id: &str, status: StageStatus) {
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = status;
    save_stage(&stage, work_dir).unwrap();
}

fn session_for(stage_id: &str, status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = status;
    session
}

/// The PID file the wrapper script writes at spawn, naming this test process
/// so the identity probe answers `VerifiedAlive`.
fn spawn_a_live_agent(work_dir: &Path, session: &Session) {
    write_test_pid_identity(work_dir, session, std::process::id()).unwrap();
}

#[test]
fn an_executing_stage_pointing_at_an_adjudicator_is_relinked_to_its_live_worker() {
    let temp = work_dir();
    let work = temp.path().join(".work");

    let mut adjudication = Session::new_adjudication("alpha");
    adjudication.status = SessionStatus::Running;
    spawn_a_live_agent(&work, &adjudication);
    save_session(&adjudication, &work).unwrap();

    let stage_session = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(&work, &stage_session);
    save_session(&stage_session, &work).unwrap();

    stage_at(&work, "alpha", StageStatus::Executing);
    crate::verify::transitions::update_stage("alpha", &work, |s| {
        s.session = Some(adjudication.id.clone());
        Ok(())
    })
    .unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.reconcile_executing_stages();

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.status, StageStatus::Executing);
    assert_eq!(after.session.as_deref(), Some(stage_session.id.as_str()));
    assert_eq!(
        orchestrator
            .active_sessions
            .get("alpha")
            .map(|s| s.id.as_str()),
        Some(stage_session.id.as_str())
    );
}

#[test]
fn an_executing_stage_with_no_live_worker_is_blocked() {
    let temp = work_dir();
    let work = temp.path().join(".work");

    let mut adjudication = Session::new_adjudication("alpha");
    adjudication.status = SessionStatus::Running;
    spawn_a_live_agent(&work, &adjudication);
    save_session(&adjudication, &work).unwrap();

    stage_at(&work, "alpha", StageStatus::Executing);
    crate::verify::transitions::update_stage("alpha", &work, |s| {
        s.session = Some(adjudication.id.clone());
        Ok(())
    })
    .unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.reconcile_executing_stages();

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.status, StageStatus::Blocked);
    assert_eq!(
        after.failure_info.map(|f| f.failure_type),
        Some(FailureType::InfrastructureError)
    );
    assert!(after.session.is_none());
}

#[test]
fn a_coherent_executing_stage_is_left_alone() {
    let temp = work_dir();
    let work = temp.path().join(".work");

    let stage_session = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(&work, &stage_session);
    save_session(&stage_session, &work).unwrap();

    stage_at(&work, "alpha", StageStatus::Executing);
    crate::verify::transitions::update_stage("alpha", &work, |s| {
        s.session = Some(stage_session.id.clone());
        Ok(())
    })
    .unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.reconcile_executing_stages();

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.status, StageStatus::Executing);
    assert_eq!(after.session.as_deref(), Some(stage_session.id.as_str()));
}
