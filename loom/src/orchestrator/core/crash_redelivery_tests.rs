//! A crash whose transition could not be persisted comes back on a later poll
//! while the daemon runs. Driven through the real monitor: a handler called by
//! hand proves nothing about redelivery.

use tempfile::TempDir;

use super::super::event_handler::EventHandler;
use super::super::merge_handler::with_read_only_stages;
use super::super::{Orchestrator, OrchestratorConfig};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::MonitorEvent;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage, update_stage};

const STAGE: &str = "crashing";

fn crash_count(events: &[MonitorEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, MonitorEvent::SessionCrashed { .. }))
        .count()
}

/// An `Executing` stage whose running session has a verified live process,
/// an orchestrator over it, and a first poll that has seen it `Running`.
fn running_stage() -> (TempDir, Orchestrator, Session) {
    running_stage_as(Session::new())
}

fn running_stage_as(mut session: Session) -> (TempDir, Orchestrator, Session) {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    let backend = SessionBackendKind::Tmux;
    write_terminal_config(&work, &TerminalConfig { backend }).unwrap();
    session.assign_to_stage(STAGE.to_string());
    session.status = SessionStatus::Running;
    session.backend = backend;
    save_session(&session, &work).unwrap();
    write_test_pid_identity(&work, &session, std::process::id()).unwrap();
    let mut stage = Stage::new(STAGE.to_string(), None);
    stage.id = STAGE.to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    save_stage(&stage, &work).unwrap();
    let config = OrchestratorConfig {
        work_dir: work,
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut orchestrator =
        Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();
    assert_eq!(crash_count(&orchestrator.monitor.poll().unwrap()), 0);
    (temp, orchestrator, session)
}

/// Replace the session's PID file with a start time no process has: dead.
fn process_vanishes(orchestrator: &Orchestrator, session: &Session) {
    let pid_file = orchestrator
        .config
        .work_dir
        .join("pids")
        .join(format!("{}-{}.pid", session.tracking_key, session.id));
    std::fs::write(pid_file, format!("{}\n{}\n", std::process::id(), u64::MAX)).unwrap();
}

#[test]
fn a_crash_whose_transition_failed_is_delivered_again_and_charged_once() {
    let (_temp, mut orchestrator, session) = running_stage();
    let work = orchestrator.config.work_dir.clone();
    process_vanishes(&orchestrator, &session);

    let failed = with_read_only_stages(&mut orchestrator, |orchestrator| {
        let events = orchestrator.monitor.poll().unwrap();
        assert_eq!(crash_count(&events), 1, "the vanished process is a crash");
        orchestrator.handle_events(events).unwrap();
    });
    if failed.is_none() {
        return; // root writes any directory, so the failure cannot be injected
    }
    let stranded = load_stage(STAGE, &work).unwrap();
    assert_eq!(stranded.status, StageStatus::Executing);
    assert_eq!(stranded.retry_count, 0);
    let record = load_session_exact(&work, &session.id).unwrap().unwrap();
    assert_eq!(record.status, SessionStatus::Crashed);

    let redelivered = orchestrator.monitor.poll().unwrap();
    assert_eq!(crash_count(&redelivered), 1, "the crash is delivered again");
    orchestrator.handle_events(redelivered).unwrap();

    let blocked = load_stage(STAGE, &work).unwrap();
    assert_eq!(blocked.status, StageStatus::Blocked);
    assert_eq!(blocked.retry_count, 1);

    assert_eq!(crash_count(&orchestrator.monitor.poll().unwrap()), 0);
    assert_eq!(load_stage(STAGE, &work).unwrap().retry_count, 1);
}

/// A respawn reuses the recovery session id while its signal exists, so the
/// crash latch must not outlive the attempt it answered: a second crash under
/// that id left the stage `Executing` with a dead session and its slot held.
#[test]
fn a_second_crash_under_a_reused_recovery_id_blocks_the_stage_again() {
    let mut recovery = Session::new();
    recovery.id = "recovery-crashing-abcd1234-1".to_string();
    let (_temp, mut orchestrator, session) = running_stage_as(recovery);
    let work = orchestrator.config.work_dir.clone();
    let signals = work.join("signals");
    std::fs::create_dir_all(&signals).unwrap();
    std::fs::write(signals.join(format!("{}.md", session.id)), "recovery brief").unwrap();

    process_vanishes(&orchestrator, &session);
    let events = orchestrator.monitor.poll().unwrap();
    assert_eq!(crash_count(&events), 1);
    orchestrator.handle_events(events).unwrap();
    assert_eq!(load_stage(STAGE, &work).unwrap().retry_count, 1);

    update_stage(STAGE, &work, |stage| stage.try_mark_queued()).unwrap();
    let queued = load_stage(STAGE, &work).unwrap();
    let (mut respawned, signal) = orchestrator.write_ahead_session(&queued, STAGE).unwrap();
    assert_eq!(respawned.id, session.id, "the signal's id is reused");
    assert!(signal.is_some());
    update_stage(STAGE, &work, |stage| {
        stage.try_mark_executing()?;
        stage.assign_session(respawned.id.clone());
        Ok(())
    })
    .unwrap();
    respawned.status = SessionStatus::Running;
    save_session(&respawned, &work).unwrap();
    write_test_pid_identity(&work, &respawned, std::process::id()).unwrap();
    assert_eq!(crash_count(&orchestrator.monitor.poll().unwrap()), 0);
    assert_eq!(crash_count(&orchestrator.monitor.poll().unwrap()), 0);

    process_vanishes(&orchestrator, &respawned);
    let events = orchestrator.monitor.poll().unwrap();
    assert_eq!(crash_count(&events), 1, "the second death is reported");
    orchestrator.handle_events(events).unwrap();

    let blocked = load_stage(STAGE, &work).unwrap();
    assert_eq!(blocked.status, StageStatus::Blocked);
    assert_eq!(blocked.retry_count, 2);
}

/// The contract writer's exit is persisted as `Crashed` but is no crash: a
/// stage still naming it must not receive one on a later poll.
#[test]
fn a_contract_writers_exit_is_never_redelivered_as_a_crash() {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    let backend = SessionBackendKind::Tmux;
    write_terminal_config(&work, &TerminalConfig { backend }).unwrap();
    let mut writer = Session::new_contract(STAGE);
    writer.status = SessionStatus::Running;
    writer.backend = backend;
    save_session(&writer, &work).unwrap();
    write_test_pid_identity(&work, &writer, std::process::id()).unwrap();
    let mut stage = Stage::new(STAGE.to_string(), None);
    stage.id = STAGE.to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(writer.id.clone());
    save_stage(&stage, &work).unwrap();
    let config = OrchestratorConfig {
        work_dir: work,
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut orchestrator =
        Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();
    assert_eq!(crash_count(&orchestrator.monitor.poll().unwrap()), 0);
    process_vanishes(&orchestrator, &writer);

    assert_eq!(crash_count(&orchestrator.monitor.poll().unwrap()), 0);
    assert_eq!(crash_count(&orchestrator.monitor.poll().unwrap()), 0);
}
