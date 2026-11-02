//! `loom stage reset` on a record with no PID identity, through the real
//! runtime and real records. Manual mode leaves such a record by design, and
//! a crash before the backend launched leaves the same one: neither may wedge
//! the stage. A launch that began may still produce a process.

use chrono::{Duration, Utc};
use tempfile::TempDir;

use super::{reset_and_cancel, OsRuntime};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{
    Session, SessionBackendKind, SessionExitReason, SessionStatus, TerminalConfig,
};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::native::{
    wrapper_script_path, write_test_pid_identity, NativeBackend,
};
use crate::verify::transitions::{load_stage, save_stage};

const STAGE: &str = "alpha";

fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    let backend = SessionBackendKind::Tmux;
    write_terminal_config(temp.path(), &TerminalConfig { backend }).unwrap();
    temp
}

fn record(work: &std::path::Path, status: SessionStatus, age_secs: i64) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(STAGE.to_string());
    session.backend = SessionBackendKind::Tmux;
    session.status = status;
    session.created_at = Utc::now() - Duration::seconds(age_secs);
    save_session(&session, work).unwrap();
    let mut stage = Stage::new(STAGE.to_string(), None);
    stage.id = STAGE.to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    save_stage(&stage, work).unwrap();
    session
}

/// What `prepare_session_launch` leaves before the backend starts anything.
fn begin_launch(work: &std::path::Path, session: &Session) {
    let (_, pid_key) = NativeBackend::window_title_and_pid_key(session).unwrap();
    let wrapper = wrapper_script_path(work, &pid_key);
    std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    std::fs::write(wrapper, "#!/bin/sh\n").unwrap();
}

fn reset(work: &std::path::Path, kill_session: bool) -> anyhow::Result<()> {
    reset_and_cancel(work, STAGE, false, kill_session, &OsRuntime::new(work))
}

fn assert_reset_and_retired(work: &std::path::Path, session: &Session) {
    let stage = load_stage(STAGE, work).unwrap();
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert_eq!(stage.session, None);
    let stored = load_session_exact(work, &session.id).unwrap().unwrap();
    assert_eq!(stored.status, SessionStatus::ContextExhausted);
    assert_eq!(stored.exit_reason, Some(SessionExitReason::OperatorStop));
}

fn assert_refused_unchanged(work: &std::path::Path, session: &Session, error: anyhow::Error) {
    let stage = load_stage(STAGE, work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing, "{error:#}");
    assert_eq!(stage.session.as_deref(), Some(session.id.as_str()));
    let stored = load_session_exact(work, &session.id).unwrap().unwrap();
    assert_eq!(stored.status, session.status);
}

#[test]
fn a_manual_record_resets_at_once() {
    for kill_session in [false, true] {
        let temp = work_dir();
        let session = record(temp.path(), SessionStatus::Spawning, 0);

        reset(temp.path(), kill_session).unwrap();

        assert_reset_and_retired(temp.path(), &session);
    }
}

/// The race RT-02 and RT-03 closed: the wrapper exists, so the process may
/// still come up and write its PID file. The reset waits for it.
#[test]
fn a_fresh_launch_that_began_stays_uncertain() {
    for kill_session in [false, true] {
        let temp = work_dir();
        let session = record(temp.path(), SessionStatus::Spawning, 0);
        begin_launch(temp.path(), &session);

        let error = reset(temp.path(), kill_session).unwrap_err();

        assert_refused_unchanged(temp.path(), &session, error);
    }
}

#[test]
fn a_launch_that_died_ends_after_the_grace_period() {
    for kill_session in [false, true] {
        let temp = work_dir();
        let session = record(temp.path(), SessionStatus::Spawning, 300);
        begin_launch(temp.path(), &session);
        // Grace runs from the wrapper, so an old launch needs an old wrapper too.
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        let aged = std::time::SystemTime::now() - std::time::Duration::from_secs(300);
        std::fs::File::options()
            .write(true)
            .open(wrapper_script_path(temp.path(), &pid_key))
            .unwrap()
            .set_modified(aged)
            .unwrap();

        reset(temp.path(), kill_session).unwrap();

        assert_reset_and_retired(temp.path(), &session);
    }
}

#[test]
fn an_old_running_record_with_no_identity_does_not_wedge() {
    for kill_session in [false, true] {
        let temp = work_dir();
        let session = record(temp.path(), SessionStatus::Running, 300);

        reset(temp.path(), kill_session).unwrap();

        assert_reset_and_retired(temp.path(), &session);
    }
}

#[test]
fn a_fresh_running_record_with_no_identity_stays_uncertain() {
    for kill_session in [false, true] {
        let temp = work_dir();
        let session = record(temp.path(), SessionStatus::Running, 0);

        let error = reset(temp.path(), kill_session).unwrap_err();

        assert_refused_unchanged(temp.path(), &session, error);
    }
}

/// The positive control: age never overrules a process that is there.
#[test]
fn an_old_reservation_with_a_live_process_refuses_a_plain_reset() {
    let temp = work_dir();
    let session = record(temp.path(), SessionStatus::Spawning, 300);
    write_test_pid_identity(temp.path(), &session, std::process::id()).unwrap();

    let error = reset(temp.path(), false).unwrap_err();

    assert!(format!("{error:#}").contains("live agent"), "{error:#}");
    assert_refused_unchanged(temp.path(), &session, error);
}
