//! Real Seatbelt and socket peers: no sandbox or identity predicate is mocked.
use super::*;
use crate::daemon::{read_message, socket_path, write_message, Request, Response};
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{CommandConfinement, Stage, StageStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec};
use crate::verify::transitions::save_stage;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

mod fixture;
use fixture::Fixture;

#[test]
fn inherited_seatbelt_child() {
    let Ok(mode) = std::env::var("LOOM_INHERITANCE_TEST") else {
        return;
    };
    let cwd = std::env::current_dir().unwrap();
    let root = PathBuf::from(std::env::var_os("LOOM_WORK_DIR").unwrap());
    let session = std::env::var("LOOM_SESSION_ID").unwrap();
    if mode != "positive" {
        assert!(!available(&cwd), "negative {mode} authorized inheritance");
        println!("INHERITANCE_REFUSED:{mode}");
        return;
    }
    assert!(kernel::sandboxed(std::process::id()));
    assert!(available(&cwd), "live protected session must inherit");
    assert!(!client::validate(&root, "stage", &session, cwd.parent().unwrap()).unwrap());
    assert!(!client::validate(&root, "other-stage", &session, &cwd).unwrap());
    let nested = Command::new("/usr/bin/sandbox-exec")
        .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
        .output()
        .unwrap();
    assert!(
        !nested.status.success(),
        "control must demonstrate Seatbelt nesting refusal"
    );
    run_inherited_command(&cwd);
    println!("INHERITED_SEATBELT_EXECUTED");
}

fn run_inherited_command(cwd: &Path) {
    let result = run_spec_with_timeout(
        &CommandSpec::shell("printf inherited > inherited-result; ! cat ../../secret; ! printf escaped > ../../secret"),
        Some(cwd), Duration::from_secs(10), CommandConfinement::Confined,
    ).unwrap();
    assert!(
        result.success,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert_eq!(
        std::fs::read_to_string(cwd.join("inherited-result")).unwrap(),
        "inherited"
    );
    let port = std::fs::read_to_string(cwd.join("port")).unwrap();
    let address = format!("127.0.0.1:{port}").parse().unwrap();
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_err());
}

#[test]
fn live_inherited_seatbelt_runs_confined_command_when_nesting_fails() {
    let fixture = Fixture::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let _control = std::net::TcpStream::connect(address).unwrap();
    std::fs::write(fixture.cwd.join("port"), address.port().to_string()).unwrap();
    let output = fixture.run("positive", true);
    assert!(output.contains("INHERITED_SEATBELT_EXECUTED"));
    assert_eq!(
        std::fs::read_to_string(fixture.repo.join("secret")).unwrap(),
        "untouched"
    );
}

#[test]
fn unsandboxed_environment_claims_cannot_authorize_inheritance() {
    let fixture = Fixture::new();
    assert!(fixture
        .run("unsandboxed", false)
        .contains("INHERITANCE_REFUSED:unsandboxed"));
    assert!(matches!(
        authorize(
            &fixture.root,
            "stage",
            &fixture.session.id,
            &fixture.cwd,
            Some(std::process::id())
        ),
        Response::Error { .. }
    ));
}

#[test]
fn missing_disabled_and_stage_writable_capsules_are_refused() {
    for mode in ["missing-capsule", "disabled-capsule", "writable-state"] {
        let fixture = Fixture::new();
        fixture.change(mode);
        assert!(fixture
            .run(mode, true)
            .contains(&format!("INHERITANCE_REFUSED:{mode}")));
    }
}

#[test]
fn fake_socket_stale_identity_and_wrong_session_are_refused() {
    for mode in [
        "fake-socket",
        "stale-daemon",
        "stale-session",
        "wrong-session",
    ] {
        let fixture = Fixture::new();
        fixture.change(mode);
        assert!(fixture
            .run(mode, true)
            .contains(&format!("INHERITANCE_REFUSED:{mode}")));
    }
}

fn serve(listener: UnixListener, root: PathBuf, stop: Arc<AtomicBool>) {
    listener.set_nonblocking(true).unwrap();
    while !stop.load(Ordering::Acquire) {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(error) => panic!("fixture accept: {error}"),
        };
        // A fake-daemon client rejects the peer before writing, closing this socket.
        if stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .is_err()
        {
            continue;
        }
        let Ok(Request::ValidateInheritedConfinement {
            stage_id,
            session_id,
            working_dir,
            ..
        }) = read_message(&mut stream)
        else {
            continue;
        };
        let peer = crate::daemon::server::peer_pid(&stream);
        let response = authorize(&root, &stage_id, &session_id, &working_dir, peer);
        write_message(&mut stream, &response).unwrap();
    }
}
