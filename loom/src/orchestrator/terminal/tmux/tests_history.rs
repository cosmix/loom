//! The terminal history a tmux session leaves behind: what `kill_session`
//! saves before it ends the server, against a real tmux server where one is
//! needed.

use super::socket::tests::{TmuxServerGuard, TmuxTmpDirGuard};
use super::*;
use crate::process::sandbox_probe::{skip_unless, unix_socket_bindable};
use serial_test::serial;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

/// How long a wait for the agent runs. A slow capture is bounded by
/// `TMUX_PROBE_TIMEOUT` and the suite runs other tests alongside, so the budget
/// is many captures long; a wait that passes returns at once.
const WAIT_BUDGET: Duration = Duration::from_secs(60);

fn tmux_installed() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Whether a test that needs a real tmux server must be skipped here.
fn skip_without_tmux(test: &str, socket_dir: &Path) -> bool {
    let usable = unix_socket_bindable(socket_dir) && tmux_installed();
    skip_unless(
        usable,
        test,
        "tmux is not installed or cannot bind a socket",
    )
}

fn staged_session() -> Session {
    let mut session = Session::new();
    session.assign_to_stage("history-stage".to_string());
    session
}

fn write_script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let script = dir.join(name);
    std::fs::write(&script, body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

/// An agent that prints more lines than tmux keeps by default, creates
/// `done_marker`, then exits.
fn write_agent(dir: &Path, done_marker: &Path) -> std::path::PathBuf {
    let body = format!(
        "#!/bin/sh\nseq 1 4000\necho AGENT-EXITED\ntouch '{}'\n",
        done_marker.display()
    );
    write_script(dir, "agent.sh", &body)
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT_BUDGET;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[serial]
fn killing_a_session_whose_agent_exited_saves_the_whole_scrollback() {
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    if skip_without_tmux(
        "killing_a_session_whose_agent_exited_saves_the_whole_scrollback",
        socket_dir.path(),
    ) {
        return;
    }
    let work = tempfile::TempDir::new().unwrap();
    let session = staged_session();
    let socket = socket_name(&session);
    let _server = TmuxServerGuard::new(&socket);
    let name = viewer::tmux_session_name(&session).unwrap();
    let done_marker = work.path().join("agent-done");
    let agent = write_agent(work.path(), &done_marker);
    spawn_in_tmux(&socket, &name, work.path(), &agent).unwrap();
    // The marker costs no tmux call; the capture then waits only for tmux to
    // finish reading what the agent printed.
    wait_until("the agent to finish", || done_marker.exists());
    wait_until("the agent's last line", || {
        capture::capture_history(&session)
            .is_some_and(|screen| String::from_utf8_lossy(&screen).contains("AGENT-EXITED"))
    });

    // The agent has exited on its own; `remain-on-exit` keeps the server and
    // its dead pane for the capture.
    assert!(
        socket_path_for(&socket).exists(),
        "the server must outlive the agent"
    );
    TmuxBackend::new(work.path().to_path_buf())
        .kill_session(&session)
        .unwrap();

    let saved = read_history(work.path(), &session.id).unwrap().unwrap();
    let text = String::from_utf8_lossy(&saved);
    assert!(text.contains("AGENT-EXITED"));
    // Line 1 is further back than tmux's default 2000 lines of scrollback.
    assert!(
        text.lines().any(|line| line == "1"),
        "the first line was lost"
    );
    assert!(
        !socket_path_for(&socket).exists(),
        "the server must be gone"
    );
}

#[test]
#[serial]
fn the_server_guard_ends_a_server_that_is_still_running() {
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    if skip_without_tmux(
        "the_server_guard_ends_a_server_that_is_still_running",
        socket_dir.path(),
    ) {
        return;
    }
    let work = tempfile::TempDir::new().unwrap();
    let session = staged_session();
    let socket = socket_name(&session);
    let name = viewer::tmux_session_name(&session).unwrap();
    let agent = write_script(work.path(), "sleeper.sh", "#!/bin/sh\nsleep 300\n");

    let guard = TmuxServerGuard::new(&socket);
    spawn_in_tmux(&socket, &name, work.path(), &agent).unwrap();
    assert!(socket_path_for(&socket).exists(), "the server must be up");
    drop(guard);

    assert!(
        !socket_path_for(&socket).exists(),
        "the guard must end the server"
    );
    let probe = run_tmux_control(
        &["-L", &socket, "has-session", "-t", &name],
        TMUX_PROBE_TIMEOUT,
        "tmux has-session after the guard",
    )
    .unwrap();
    assert!(!probe.status.success(), "no server may answer");
}

#[test]
#[serial]
fn killing_a_session_with_no_server_saves_nothing_and_still_succeeds() {
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    let work = tempfile::TempDir::new().unwrap();
    let session = staged_session();

    TmuxBackend::new(work.path().to_path_buf())
        .kill_session(&session)
        .unwrap();

    assert!(!has_history(work.path(), &session.id));
}

#[test]
#[serial]
fn a_second_kill_leaves_the_first_history_alone() {
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    let work = tempfile::TempDir::new().unwrap();
    let session = staged_session();
    write_history(work.path(), &session.id, b"saved by the first kill\n").unwrap();

    TmuxBackend::new(work.path().to_path_buf())
        .kill_session(&session)
        .unwrap();

    assert_eq!(
        read_history(work.path(), &session.id).unwrap().as_deref(),
        Some(&b"saved by the first kill\n"[..])
    );
}
