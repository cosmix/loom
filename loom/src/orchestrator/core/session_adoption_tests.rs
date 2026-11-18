//! Unit tests for [`super::Orchestrator::adopt_live_session_if_present`]'s
//! worker-kind lookup and its contract-phase fallback, and for the on-disk
//! status gate `start_stage` puts in front of it.
//!
//! Fixtures mirror `coherence_tests.rs`: they are private to that module, so
//! the same small set is duplicated here.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::{Stage, StageType};
use crate::orchestrator::core::stage_executor::StageExecutor;
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::schema::StageDefinition;
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

/// An orchestrator whose graph holds `alpha` as `Queued`, whatever its file
/// says: what `start_stage` sees when the file moved on after the tick's
/// graph sync.
fn orchestrator_for(work_dir: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let graph = ExecutionGraph::build(vec![StageDefinition {
        id: "alpha".to_string(),
        name: "alpha".to_string(),
        working_dir: ".".to_string(),
        ..Default::default()
    }])
    .unwrap();
    let mut orchestrator = Orchestrator::new(config, graph).unwrap();
    orchestrator
        .graph
        .force_status("alpha", StageStatus::Queued)
        .unwrap();
    orchestrator
}

fn queued_stage(work_dir: &Path, stage_type: StageType, plan_version: u32) {
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Queued;
    stage.stage_type = stage_type;
    stage.plan_version = plan_version;
    save_stage(&stage, work_dir).unwrap();
}

/// Stage `alpha` in `status`, naming `session` as its agent.
fn stage_in(work_dir: &Path, status: StageStatus, session: Option<&str>) {
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = status;
    stage.session = session.map(str::to_string);
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
    assert_eq!(
        orchestrator.adopt_live_session_if_present("alpha").unwrap(),
        super::AdoptionResult::Adopted
    );

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
    assert_eq!(
        orchestrator.adopt_live_session_if_present("alpha").unwrap(),
        super::AdoptionResult::Adopted
    );
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
        assert_eq!(
            orchestrator.adopt_live_session_if_present("alpha").unwrap(),
            super::AdoptionResult::Absent
        );
        let after = load_stage("alpha", &work).unwrap();
        assert_eq!(
            after.status,
            StageStatus::Queued,
            "v{plan_version} {stage_type:?}"
        );
    }
}

#[test]
fn malformed_only_session_inventory_prevents_replacement_spawn() {
    let temp = work_dir();
    let work = temp.path().join(".work");
    queued_stage(&work, StageType::Standard, 1);
    let sessions = work.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("unreadable-worker.md"), "malformed").unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    assert!(matches!(
        orchestrator.start_stage("alpha").unwrap(),
        super::super::stage_executor::StartDisposition::Deferred
    ));

    let stage = load_stage("alpha", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(stage.session.is_none());
    assert!(orchestrator.active_sessions.is_empty());
}

/// A stage the graph still holds `Queued` but whose file has moved on is not
/// the executor's to start: adopting its live agent would walk it back to
/// `Executing` out of a status it was put in on purpose, the way an escalated
/// dispute's idle worker was re-adopted. The graph node follows the file.
#[test]
fn start_stage_leaves_a_stage_whose_file_is_not_startable_alone() {
    for status in [
        StageStatus::NeedsHumanReview,
        StageStatus::NeedsAdjudication,
        StageStatus::WaitingForInput,
        StageStatus::CompletedWithFailures,
        StageStatus::MergeBlocked,
        StageStatus::Blocked,
        StageStatus::NeedsHandoff,
    ] {
        let temp = work_dir();
        let work = temp.path().join(".work");
        let worker = live_session(&work, stage_session());
        stage_in(&work, status.clone(), Some(worker.id.as_str()));

        let mut orchestrator = orchestrator_for(&work, temp.path());
        orchestrator.start_stage("alpha").unwrap();

        let after = load_stage("alpha", &work).unwrap();
        assert_eq!(after.status, status, "the file's status must stand");
        assert_eq!(
            after.session.as_deref(),
            Some(worker.id.as_str()),
            "{status:?}"
        );
        assert!(orchestrator.active_sessions.is_empty(), "{status:?}");
        assert_eq!(
            orchestrator
                .graph
                .get_node("alpha")
                .map(|n| n.status.clone()),
            Some(status.clone()),
            "the graph node must follow the file"
        );
    }
}

/// Adoption walks only from `Queued`, so a `WaitingForDeps` stage with a live
/// worker is adopted once the dependency check has stepped it there, instead
/// of failing to adopt on every tick.
#[test]
fn start_stage_adopts_a_live_worker_once_a_waiting_stage_is_queued() {
    let temp = work_dir();
    let work = temp.path().join(".work");
    let worker = live_session(&work, stage_session());
    stage_in(&work, StageStatus::WaitingForDeps, None);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.start_stage("alpha").unwrap();

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.status, StageStatus::Executing);
    assert_eq!(after.session.as_deref(), Some(worker.id.as_str()));
    assert_eq!(
        orchestrator
            .active_sessions
            .get("alpha")
            .map(|s| s.id.as_str()),
        Some(worker.id.as_str())
    );
}
