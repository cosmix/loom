//! A resolver's launch reservation ends: a `Spawning` record whose launch died
//! must not hold its stage for good, and one that still may produce a process
//! must keep it.

use chrono::{Duration, Utc};
use tempfile::TempDir;

use super::find_live_merge_session_for_stage;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::Stage;
use crate::orchestrator::signals::generate_merge_signal;
use crate::orchestrator::terminal::native::write_test_pid_identity;

const STAGE: &str = "alpha";

fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    let backend = SessionBackendKind::Tmux;
    write_terminal_config(temp.path(), &TerminalConfig { backend }).unwrap();
    temp
}

fn reservation(work: &std::path::Path, status: SessionStatus, age_secs: i64) -> Session {
    let mut session = Session::new_merge("loom/alpha".into(), "main".into());
    session.assign_to_stage(STAGE.to_string());
    session.backend = SessionBackendKind::Tmux;
    session.status = status;
    session.created_at = Utc::now() - Duration::seconds(age_secs);
    let mut stage = Stage::new(STAGE.to_string(), None);
    stage.id = STAGE.to_string();
    generate_merge_signal(&session, &stage, "loom/alpha", "main", &[], work).unwrap();
    save_session(&session, work).unwrap();
    session
}

fn signal(work: &std::path::Path, session: &Session) -> std::path::PathBuf {
    work.join("signals").join(format!("{}.md", session.id))
}

#[test]
fn a_dead_launch_reservation_ends_and_is_retired() {
    let temp = work_dir();
    let work = temp.path();
    let session = reservation(work, SessionStatus::Spawning, 300);

    assert_eq!(
        find_live_merge_session_for_stage(STAGE, work).unwrap(),
        None
    );

    assert!(!signal(work, &session).exists());
    let stored = load_session_exact(work, &session.id).unwrap().unwrap();
    assert!(stored.status.is_terminal(), "{:?}", stored.status);
}

#[test]
fn a_fresh_reservation_keeps_its_stage_and_signal() {
    let temp = work_dir();
    let work = temp.path();
    let session = reservation(work, SessionStatus::Spawning, 0);

    assert_eq!(
        find_live_merge_session_for_stage(STAGE, work).unwrap(),
        Some(session.id.clone())
    );

    assert!(signal(work, &session).exists());
    let stored = load_session_exact(work, &session.id).unwrap().unwrap();
    assert_eq!(stored.status, SessionStatus::Spawning);
}

#[test]
fn an_old_reservation_whose_process_runs_keeps_its_stage() {
    let temp = work_dir();
    let work = temp.path();
    let session = reservation(work, SessionStatus::Spawning, 300);
    write_test_pid_identity(work, &session, std::process::id()).unwrap();

    assert_eq!(
        find_live_merge_session_for_stage(STAGE, work).unwrap(),
        Some(session.id)
    );
}

#[test]
fn an_old_running_record_with_no_identity_does_not_hold_the_stage() {
    let temp = work_dir();
    let work = temp.path();
    let session = reservation(work, SessionStatus::Running, 300);

    assert_eq!(
        find_live_merge_session_for_stage(STAGE, work).unwrap(),
        None
    );
    assert!(!signal(work, &session).exists());
}
