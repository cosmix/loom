//! Tests for the contract phase: the `Contract` session a v2 standard stage
//! starts with, and its hand-over to the `Stage` session.
//!
//! The orchestrator runs in manual mode, so a spawn writes the session record
//! and the signal and assigns the session without launching a terminal. The
//! contract agent's kill is observed through a real process, as the other
//! takedown tests do.

use std::path::Path;

use serial_test::serial;

use super::super::tests::{
    create_test_graph, handoff_work_dir, spawn_orphan_process, write_pid_file,
};
use super::super::EventHandler;
use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::SessionBackendKind;
use crate::models::stage::StageType;
use crate::orchestrator::core::stage_executor::StageExecutor;
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::contracts::test_support::{contract, contract_worktree, write_test_freeze};
use crate::verify::transitions::{create_stage, load_stage};

/// Points `LOOM_HOOKS_DIR` at an existing directory for the test and restores
/// the previous value on drop, panics included. A spawn refuses a host with
/// no hooks directory, and the ambient `~/.claude/hooks/loom` is absent on CI.
struct HooksDirGuard {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl HooksDirGuard {
    fn install() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::var_os("LOOM_HOOKS_DIR");
        std::env::set_var("LOOM_HOOKS_DIR", dir.path());
        Self {
            _dir: dir,
            original,
        }
    }
}

impl Drop for HooksDirGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("LOOM_HOOKS_DIR", value),
            None => std::env::remove_var("LOOM_HOOKS_DIR"),
        }
    }
}

fn manual_orchestrator(work_dir: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        manual_mode: true,
        ..Default::default()
    };
    Orchestrator::new(config, create_test_graph()).unwrap()
}

fn v2_stage(status: StageStatus, session: Option<&str>) -> Stage {
    Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status,
        session: session.map(str::to_string),
        working_dir: Some(".".to_string()),
        plan_version: 2,
        stage_type: StageType::Standard,
        contracts: vec![contract()],
        ..Stage::default()
    }
}

/// A running contract session for `test-stage`, recorded on disk.
fn contract_session(work_dir: &Path) -> Session {
    let mut session = Session::new_contract("test-stage");
    session.status = SessionStatus::Running;
    session.backend = SessionBackendKind::Tmux;
    save_session(&session, work_dir).unwrap();
    session
}

/// The worktree handle the executor keeps for a running stage.
fn track_worktree(orchestrator: &mut Orchestrator, repo_root: &Path) {
    let path = repo_root.join(".worktrees").join("test-stage");
    std::fs::create_dir_all(&path).unwrap();
    let worktree = Worktree::new(
        "test-stage".to_string(),
        path,
        "loom/test-stage".to_string(),
    );
    orchestrator
        .active_worktrees
        .insert("test-stage".to_string(), worktree);
}

fn assigned_session(work_dir: &Path, stage: &Stage) -> Session {
    let id = stage.session.as_deref().expect("the stage names its agent");
    load_session_exact(work_dir, id)
        .unwrap()
        .expect("the assigned session has a record")
}

#[test]
#[serial]
fn contract_session_spawns_before_stage_session() {
    let _hooks = HooksDirGuard::install();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    contract_worktree(temp.path(), "test-stage");
    create_stage(&v2_stage(StageStatus::Queued, None), &work).unwrap();
    let mut orchestrator = manual_orchestrator(&work, temp.path());

    orchestrator.start_stage("test-stage").unwrap();

    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    let session = assigned_session(&work, &stage);
    assert_eq!(
        session.session_type,
        SessionType::Contract,
        "a v2 standard stage with unfrozen contracts starts with its contract writer"
    );
    assert!(work
        .join("signals")
        .join(format!("{}.md", session.id))
        .exists());
    assert_eq!(orchestrator.active_sessions["test-stage"].id, session.id);
}

#[test]
#[serial]
fn contract_phase_finished_spawns_stage_session() {
    let _hooks = HooksDirGuard::install();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let writer = contract_session(&work);
    create_stage(&v2_stage(StageStatus::Executing, Some(&writer.id)), &work).unwrap();
    write_test_freeze(&work, "test-stage", &writer.id);
    let mut orchestrator = manual_orchestrator(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    track_worktree(&mut orchestrator, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), writer.clone());

    // A contract agent the kill cannot take down keeps the stage as it is.
    // The PID file is identity evidence, so manual mode still takes it down.
    write_pid_file(&work, &writer, None);
    orchestrator
        .on_contract_phase_finished("test-stage", &writer.id)
        .unwrap();
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.session.as_deref(), Some(writer.id.as_str()));

    // Once it can be taken down, the implementation session takes over.
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &writer, agent_pid).unwrap();
    orchestrator
        .on_contract_phase_finished("test-stage", &writer.id)
        .unwrap();

    assert!(
        !crate::process::is_process_alive(agent_pid),
        "the contract agent must be killed before the implementer starts"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    let successor = assigned_session(&work, &stage);
    assert_ne!(successor.id, writer.id);
    assert_eq!(successor.session_type, SessionType::Stage);
    assert_eq!(orchestrator.active_sessions["test-stage"].id, successor.id);
}

/// Manual mode, writer started by hand: loom holds no PID identity for it, so
/// the freeze hands the stage to the implementer at once instead of deferring
/// on every poll, and the writer's record and signal are closed.
#[test]
#[serial]
fn manual_handover_without_writer_identity_starts_implementer() {
    let _hooks = HooksDirGuard::install();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let writer = contract_session(&work);
    let writer_signal = work.join("signals").join(format!("{}.md", writer.id));
    std::fs::create_dir_all(writer_signal.parent().unwrap()).unwrap();
    std::fs::write(&writer_signal, "# Contract Signal\n").unwrap();
    create_stage(&v2_stage(StageStatus::Executing, Some(&writer.id)), &work).unwrap();
    write_test_freeze(&work, "test-stage", &writer.id);
    let mut orchestrator = manual_orchestrator(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    track_worktree(&mut orchestrator, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), writer.clone());

    orchestrator
        .on_contract_phase_finished("test-stage", &writer.id)
        .unwrap();

    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    let successor = assigned_session(&work, &stage);
    assert_ne!(successor.id, writer.id);
    assert_eq!(successor.session_type, SessionType::Stage);
    assert_eq!(orchestrator.active_sessions["test-stage"].id, successor.id);
    let released = load_session_exact(&work, &writer.id).unwrap().unwrap();
    assert!(
        released.status.is_terminal(),
        "a writer left in progress would read as a survivor to every later takedown"
    );
    assert!(!writer_signal.exists());
}

/// A contract writer at its context ceiling runs `loom handoff`, leaving its
/// stage `NeedsHandoff`. Nothing is frozen yet, so the continuation must be
/// another contract writer: a `Stage` session would skip the contract phase.
#[test]
#[serial]
fn contract_writer_ceiling_handoff_continues_as_contract_writer() {
    let _hooks = HooksDirGuard::install();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    contract_worktree(temp.path(), "test-stage");
    let writer = contract_session(&work);
    create_stage(
        &v2_stage(StageStatus::NeedsHandoff, Some(&writer.id)),
        &work,
    )
    .unwrap();
    // A start time no process could have: the takedown probes the writer dead.
    write_pid_file(&work, &writer, Some(u64::MAX));
    let mut orchestrator = manual_orchestrator(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();

    orchestrator
        .on_needs_handoff(&writer.id, "test-stage")
        .unwrap();
    let requeued = load_stage("test-stage", &work).unwrap();
    assert_eq!(requeued.status, StageStatus::Queued);
    orchestrator.start_stage("test-stage").unwrap();

    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    let successor = assigned_session(&work, &stage);
    assert_ne!(successor.id, writer.id);
    assert_eq!(successor.session_type, SessionType::Contract);
    let signal_path = work.join("signals").join(format!("{}.md", successor.id));
    let signal = std::fs::read_to_string(signal_path).unwrap();
    assert!(signal.contains("# Contract Signal:"), "{signal}");
}

#[test]
#[serial]
fn contract_session_exit_without_freeze_respawns() {
    let _hooks = HooksDirGuard::install();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let first = contract_session(&work);
    create_stage(&v2_stage(StageStatus::Executing, Some(&first.id)), &work).unwrap();
    let mut orchestrator = manual_orchestrator(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    track_worktree(&mut orchestrator, temp.path());

    let mut ended = first.id;
    for spent in 1..=3 {
        orchestrator
            .on_contract_session_ended("test-stage", &ended)
            .unwrap();
        let stage = load_stage("test-stage", &work).unwrap();
        assert_eq!(stage.status, StageStatus::Executing);
        let replacement = assigned_session(&work, &stage);
        assert_ne!(replacement.id, ended);
        assert_eq!(replacement.session_type, SessionType::Contract);
        assert_eq!(attempts_spent(&work, "test-stage").unwrap(), spent);
        ended = replacement.id;
    }

    orchestrator
        .on_contract_session_ended("test-stage", &ended)
        .unwrap();
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(
        stage.review_reason.as_deref(),
        Some("contract session ended 3 times without freezing contracts")
    );
    assert_eq!(stage.session, None);
    assert_eq!(attempts_spent(&work, "test-stage").unwrap(), 3);
}
