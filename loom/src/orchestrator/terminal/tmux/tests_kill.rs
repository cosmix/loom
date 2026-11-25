//! What `kill_session` does with a session whose tmux server is already gone
//! (no socket), by the PID evidence stored for it: a verified process is
//! signalled, an unverifiable one is left alone, and either way the kill
//! reports success.

use super::*;
use crate::test_stand_in::{require_signals, StandIn};
use std::time::Duration;

/// A stage session with no tmux socket, and the stand-in agent process the
/// stored PID entry of `entry` names. `entry` receives the pid and returns the
/// PID file's contents.
fn session_with_stored_pid(work: &Path, entry: impl Fn(u32) -> String) -> (Session, StandIn) {
    let mut session = Session::new();
    session.assign_to_stage("kill-stage".to_string());
    let agent = StandIn::direct(std::process::Command::new("sleep").arg("300"));
    let (_, pid_key) = launch::tracking_and_pid_key(&session).unwrap();
    let pids = work.join("pids");
    std::fs::create_dir_all(&pids).unwrap();
    std::fs::write(pids.join(format!("{pid_key}.pid")), entry(agent.id())).unwrap();
    assert!(
        !socket_path_for(&socket_name(&session)).exists(),
        "the session must have no tmux socket"
    );
    (session, agent)
}

#[test]
fn a_verified_process_is_signalled_when_no_tmux_socket_exists() {
    require_signals!();
    let work = tempfile::TempDir::new().unwrap();
    let (session, mut agent) = session_with_stored_pid(work.path(), |pid| {
        let start = crate::process::process_start_time(pid).expect("start time of a live child");
        format!("{pid}\n{start}\n")
    });

    TmuxBackend::new(work.path().to_path_buf())
        .kill_session(&session)
        .unwrap();

    let status = agent.wait_within(Duration::from_secs(30));
    assert!(!status.success(), "the agent must die of the signal");
}

#[test]
fn an_unverifiable_process_is_left_alone_when_no_tmux_socket_exists() {
    require_signals!();
    let work = tempfile::TempDir::new().unwrap();
    let (session, mut agent) = session_with_stored_pid(work.path(), |pid| format!("{pid}\n"));

    TmuxBackend::new(work.path().to_path_buf())
        .kill_session(&session)
        .unwrap();

    std::thread::sleep(Duration::from_millis(300));
    assert!(
        agent.child().try_wait().unwrap().is_none(),
        "a PID entry with no start time must not be signalled"
    );
}
