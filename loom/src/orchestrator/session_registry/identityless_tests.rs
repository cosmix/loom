//! A record with no PID identity stays uncertain while a process may still
//! come up, and ends once none can.

use chrono::{Duration, Utc};
use tempfile::TempDir;

use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::orchestrator::session_registry::{
    assigned_session_is_live, assigned_session_is_live_admitted, live_sessions_for_stage,
};
use crate::orchestrator::terminal::native::{
    wrapper_script_path, write_test_pid_identity, NativeBackend,
};

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
    session
}

#[test]
fn an_old_record_with_no_identity_is_not_live() {
    for status in [SessionStatus::Spawning, SessionStatus::Running] {
        let temp = work_dir();
        let session = record(temp.path(), status, 300);

        assert!(live_sessions_for_stage(temp.path(), STAGE)
            .unwrap()
            .is_empty());
        assert!(!assigned_session_is_live(temp.path(), STAGE, &session.id).unwrap());
    }
}

#[test]
fn a_fresh_record_with_no_identity_stays_uncertain() {
    for status in [SessionStatus::Spawning, SessionStatus::Running] {
        let temp = work_dir();
        let session = record(temp.path(), status, 0);

        assert!(live_sessions_for_stage(temp.path(), STAGE).is_err());
        assert!(assigned_session_is_live(temp.path(), STAGE, &session.id).is_err());
    }
}

#[test]
fn age_never_overrules_a_process_that_is_there() {
    let temp = work_dir();
    let session = record(temp.path(), SessionStatus::Spawning, 300);
    write_test_pid_identity(temp.path(), &session, std::process::id()).unwrap();

    let live = live_sessions_for_stage(temp.path(), STAGE).unwrap();

    assert_eq!(live.len(), 1);
    assert!(assigned_session_is_live(temp.path(), STAGE, &session.id).unwrap());
}

/// Overlay reconcile and signal generation run between the record and the
/// wrapper, so an old record says nothing about a launch that just began.
#[test]
fn the_grace_runs_from_the_wrapper_script_when_one_exists() {
    let temp = work_dir();
    let session = record(temp.path(), SessionStatus::Spawning, 300);
    let wrapper = write_wrapper(temp.path(), &session);

    assert!(assigned_session_is_live(temp.path(), STAGE, &session.id).is_err());
    assert!(live_sessions_for_stage(temp.path(), STAGE).is_err());

    let aged = std::time::SystemTime::now() - std::time::Duration::from_secs(300);
    std::fs::File::options()
        .write(true)
        .open(&wrapper)
        .unwrap()
        .set_modified(aged)
        .unwrap();
    assert!(!assigned_session_is_live(temp.path(), STAGE, &session.id).unwrap());
    assert!(live_sessions_for_stage(temp.path(), STAGE)
        .unwrap()
        .is_empty());
}

/// Holding admission rules out a launch in flight, which only ends a `Spawning`
/// record that never reached a backend; every other young record stays uncertain.
#[test]
fn holding_admission_ends_only_a_record_no_launch_reached() {
    let temp = work_dir();
    let unlaunched = record(temp.path(), SessionStatus::Spawning, 0);
    assert!(assigned_session_is_live(temp.path(), STAGE, &unlaunched.id).is_err());
    assert!(!assigned_session_is_live_admitted(temp.path(), STAGE, &unlaunched.id).unwrap());

    let launched = record(temp.path(), SessionStatus::Spawning, 0);
    write_wrapper(temp.path(), &launched);
    assert!(assigned_session_is_live_admitted(temp.path(), STAGE, &launched.id).is_err());

    let running = record(temp.path(), SessionStatus::Running, 0);
    assert!(assigned_session_is_live_admitted(temp.path(), STAGE, &running.id).is_err());
}

fn write_wrapper(work: &std::path::Path, session: &Session) -> std::path::PathBuf {
    let (_, key) = NativeBackend::window_title_and_pid_key(session).unwrap();
    let wrapper = wrapper_script_path(work, &key);
    std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    std::fs::write(&wrapper, "#!/bin/sh\n").unwrap();
    wrapper
}
