//! Unit tests for [`super::Orchestrator::adopt_live_session_if_present`]'s
//! worker-kind lookup and its contract-phase fallback.
//!
//! Fixtures mirror `coherence_tests.rs`: they are private to that module, so
//! the same small set is duplicated here.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::{Stage, StageType};
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage};
use std::path::Path;
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

fn queued_stage(work_dir: &Path, stage_type: StageType, plan_version: u32) {
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Queued;
    stage.stage_type = stage_type;
    stage.plan_version = plan_version;
    save_stage(&stage, work_dir).unwrap();
}

/// A live, recorded session for stage `alpha`: the PID file names this test
/// process so the identity probe answers `VerifiedAlive`.
fn live_session(work_dir: &Path, mut session: Session) -> Session {
    session.status = SessionStatus::Running;
    write_test_pid_identity(work_dir, &session, std::process::id()).unwrap();
    save_session(&session, work_dir).unwrap();
    session
}

fn stage_session() -> Session {
    let mut session = Session::new();
    session.assign_to_stage("alpha".to_string());
    session
}

/// A live contract writer is a v2 standard stage's agent: spawning over it
/// would put two agents in one worktree.
#[test]
fn a_live_contract_session_is_adopted_for_a_v2_standard_stage() {
    let temp = work_dir();
    let work = temp.path().join(".work");
    queued_stage(&work, StageType::Standard, 2);
    let contract = live_session(&work, Session::new_contract("alpha"));

    let mut orchestrator = orchestrator_for(&work, temp.path());
    assert!(orchestrator.adopt_live_session_if_present("alpha").unwrap());

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.status, StageStatus::Executing);
    assert_eq!(after.session.as_deref(), Some(contract.id.as_str()));
    assert_eq!(
        orchestrator
            .active_sessions
            .get("alpha")
            .map(|s| s.id.as_str()),
        Some(contract.id.as_str())
    );
}

/// The contract writer is only a fallback: a live `Stage` session is the
/// stage's worker even when a contract session is also alive.
#[test]
fn a_live_stage_session_wins_over_a_live_contract_session() {
    let temp = work_dir();
    let work = temp.path().join(".work");
    queued_stage(&work, StageType::Standard, 2);
    let worker = live_session(&work, stage_session());
    live_session(&work, Session::new_contract("alpha"));

    let mut orchestrator = orchestrator_for(&work, temp.path());
    assert!(orchestrator.adopt_live_session_if_present("alpha").unwrap());
    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.session.as_deref(), Some(worker.id.as_str()));
}

/// v1 stages and non-standard stages have no contract phase: a contract
/// session is never adopted into their worker slot.
#[test]
fn a_contract_session_is_never_adopted_without_a_contract_phase() {
    for (stage_type, plan_version) in [(StageType::Standard, 1), (StageType::Knowledge, 2)] {
        let temp = work_dir();
        let work = temp.path().join(".work");
        queued_stage(&work, stage_type, plan_version);
        live_session(&work, Session::new_contract("alpha"));

        let mut orchestrator = orchestrator_for(&work, temp.path());
        assert!(!orchestrator.adopt_live_session_if_present("alpha").unwrap());
        let after = load_stage("alpha", &work).unwrap();
        assert_eq!(
            after.status,
            StageStatus::Queued,
            "v{plan_version} {stage_type:?}"
        );
    }
}
