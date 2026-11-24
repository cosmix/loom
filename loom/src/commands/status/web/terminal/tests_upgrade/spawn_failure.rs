//! The server's own close wiring for a failed attach: `start_bridge` must
//! answer a target that resolves but cannot be spawned with the attach-failed
//! close code, not hand the socket to the bridge or close with another code.

use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serial_test::serial;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::Message;

use crate::commands::status::web::tests::{skip_without_loopback, start_with, stop, workspace};
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionBackendKind, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::orchestrator::terminal::tmux::socket::tests::TmuxTmpDirGuard;
use crate::orchestrator::terminal::tmux::{socket_name, socket_path_for};
use crate::process::sandbox_probe::skip_without_pty;
use crate::verify::transitions::create_stage;

use super::super::protocol::CLOSE_ATTACH_FAILED;
use super::super::tmux_program::OverrideGuard;
use super::super::upgrade::SPAWN_FAILED;
use super::{connect_terminal, terminal_options};

/// The stage id `connect_terminal` requests.
const STAGE_ID: &str = "missing";

/// A `tmux` stand-in that answers the readiness probe (`has-session`) and then
/// deletes itself, so the attach client the server starts next finds nothing
/// to execute. Returns its path, which the test routes this module's tmux
/// program to; the process `PATH` stays untouched.
fn self_removing_tmux(dir: &Path) -> PathBuf {
    let tmux = dir.join("tmux");
    std::fs::write(
        &tmux,
        "#!/bin/sh\nPATH=/usr/bin:/bin rm -- \"$0\"\nexit 0\n",
    )
    .expect("write the tmux stand-in");
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755))
        .expect("make the tmux stand-in executable");
    tmux
}

/// A live tmux-hosted session for `STAGE_ID` whose socket file exists, so
/// the server resolves the stage to an attach target.
fn resolvable_stage(work_root: &Path) {
    create_stage(&stage(), work_root).expect("create the stage");
    let mut session = Session::new();
    session.assign_to_stage(STAGE_ID.to_owned());
    session.backend = SessionBackendKind::Tmux;
    session.status = SessionStatus::Running;
    session.pid = Some(std::process::id());
    save_session(&session, work_root).expect("save the session");
    write_test_pid_identity(work_root, &session, std::process::id()).expect("record the pid");
    let socket = socket_path_for(&socket_name(&session));
    std::fs::create_dir_all(socket.parent().expect("socket directory")).expect("socket dir");
    std::fs::write(&socket, "").expect("socket placeholder");
}

fn stage() -> Stage {
    let mut stage = Stage::new(STAGE_ID.to_owned(), None);
    stage.id = STAGE_ID.to_owned();
    stage
}

#[test]
#[serial]
fn an_attach_that_cannot_spawn_closes_4010_with_the_spawn_reason() {
    let name = "an_attach_that_cannot_spawn_closes_4010_with_the_spawn_reason";
    if skip_without_loopback(name) || skip_without_pty(name) {
        return;
    }
    let (_temp, base) = workspace();
    let work_root = WorkDir::new(&base).expect("work dir").root().to_path_buf();
    let socket_dir = tempfile::tempdir().expect("socket directory");
    let bin = tempfile::tempdir().expect("stand-in directory");
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    resolvable_stage(&work_root);
    let _tmux_program = OverrideGuard::set(self_removing_tmux(bin.path()));

    let (port, running, _) = start_with(base, terminal_options());
    let mut client = connect_terminal(port, &"a".repeat(64));
    client
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("set client read timeout");
    let deadline = Instant::now() + Duration::from_secs(10);
    let frame = loop {
        assert!(
            Instant::now() < deadline,
            "no close frame within the deadline"
        );
        match client.read() {
            Ok(Message::Close(Some(frame))) => break frame,
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(other) => panic!("expected a close frame, got {other:?}"),
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => panic!("reading the terminal socket: {error}"),
        }
    };
    stop(running);

    assert!(
        !bin.path().join("tmux").exists(),
        "the readiness probe never ran, so the target did not resolve"
    );
    assert_eq!(frame.code, CloseCode::Library(CLOSE_ATTACH_FAILED));
    assert_eq!(frame.reason.as_str(), SPAWN_FAILED);
}
