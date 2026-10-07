use super::*;
use crate::daemon::SOCKET_FILE;
use tempfile::TempDir;

#[test]
fn a_missing_user_token_yields_the_peer_identity_placeholder() {
    let temp = TempDir::new().unwrap();

    // Non-empty, because the wire preface refuses to frame an empty
    // credential — and not the token, because there is none to read.
    assert_eq!(PEER_IDENTITY_CREDENTIAL, user_credential(temp.path()));
}

#[test]
fn a_readable_user_token_is_presented_verbatim() {
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("user.token"), "user-secret\n").unwrap();

    assert_eq!("user-secret", user_credential(temp.path()));
}

fn ping() -> Request {
    Request::Ping {
        auth_token: "irrelevant".to_string(),
    }
}

/// Whether this sandbox denies AF_UNIX `connect` outright.
///
/// Outside any sandbox, connecting to a path with nothing bound answers
/// `NotFound`, and connecting to a plain (non-socket) file answers
/// `ENOTSOCK` on macOS (XNU's `unp_connect` rejects a non-socket path;
/// Linux answers `ECONNREFUSED` for the same case instead). Only a
/// sandbox that denies the `connect()` syscall itself answers
/// `PermissionDenied`, which is what this probes for.
fn af_unix_connect_denied() -> bool {
    let temp = TempDir::new().unwrap();
    matches!(
        UnixStream::connect(temp.path().join("probe.sock")),
        Err(e) if e.kind() == ErrorKind::PermissionDenied
    )
}

/// Deliberately NOT guarded by either probe above: the pre-check answers
/// this before any socket syscall, so it must hold identically sandboxed
/// and not.
/// That equivalence is the whole point of the pre-check.
#[test]
fn no_socket_file_at_all_is_not_listening() {
    let temp = TempDir::new().unwrap();

    match try_send_request(temp.path(), &ping()).unwrap() {
        DaemonReach::NotListening => {}
        DaemonReach::Answered(response) => panic!("expected NotListening, got {response:?}"),
        DaemonReach::Unreachable => panic!("expected NotListening, got Unreachable"),
    }
}

/// A socket path this process may not `lstat` (its directory is mode
/// 000) is no evidence of absence. Root ignores the mode, so there the
/// test proves nothing and returns.
#[test]
fn a_socket_path_that_cannot_be_statted_is_unreachable() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: geteuid has no arguments and only reads process credentials.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    std::fs::create_dir(&work_dir).unwrap();
    std::fs::set_permissions(&work_dir, std::fs::Permissions::from_mode(0o000)).unwrap();

    let reach = try_send_request(&work_dir, &ping());
    std::fs::set_permissions(&work_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    match reach.unwrap() {
        DaemonReach::Unreachable => {}
        DaemonReach::NotListening => panic!("a denied lstat proves no daemon absent"),
        DaemonReach::Answered(response) => panic!("expected Unreachable, got {response:?}"),
    }
}

#[test]
fn a_stale_socket_file_with_nothing_bound_is_not_listening() {
    let temp = TempDir::new().unwrap();
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::unix_socket_bindable(temp.path()),
        "daemon::rpc::tests::a_stale_socket_file_with_nothing_bound_is_not_listening",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    // A REAL stale socket, not a plain file: a plain file answers
    // ENOTSOCK on macOS (XNU's unp_connect rejects a non-socket path)
    // rather than the ECONNREFUSED a dead daemon actually produces.
    // Binding and immediately dropping the listener leaves a
    // socket-typed file on disk with nothing accepting on it, which is
    // exactly what a daemon that died without unlinking its socket
    // leaves behind. The listener is shut down before the drop: a process
    // another test forks meanwhile inherits a copy of the fd that would
    // keep accepting until its exec, and a shut-down listener refuses.
    let listener = std::os::unix::net::UnixListener::bind(socket_path(temp.path())).unwrap();
    // SAFETY: shutdown(2) on an fd this test owns and keeps open until the drop below.
    unsafe { libc::shutdown(std::os::fd::AsRawFd::as_raw_fd(&listener), libc::SHUT_RDWR) };
    drop(listener);

    match try_send_request(temp.path(), &ping()).unwrap() {
        DaemonReach::NotListening => {}
        DaemonReach::Answered(response) => panic!("expected NotListening, got {response:?}"),
        DaemonReach::Unreachable => panic!("expected NotListening, got Unreachable"),
    }
}

/// The inverse of the guard above: this one can ONLY say something where
/// AF_UNIX connect is denied, which is exactly the sandboxed stage agent
/// this variant exists for. Under a normal environment the same connect
/// fails `ENOTSOCK` against a plain file (or `ConnectionRefused` against
/// a real stale socket), which the other tests already cover.
///
/// The socket file has to exist, or the pre-check would answer
/// `NotListening` before the connect this test is about ever runs.
#[test]
fn a_sandbox_denying_af_unix_is_unreachable_not_not_listening() {
    if !af_unix_connect_denied() {
        return;
    }
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("orchestrator.sock"), b"").unwrap();

    match try_send_request(temp.path(), &ping()).unwrap() {
        DaemonReach::Unreachable => {}
        DaemonReach::NotListening => {
            panic!("a denied socket syscall says nothing about whether a daemon is listening")
        }
        DaemonReach::Answered(response) => panic!("expected Unreachable, got {response:?}"),
    }
}

#[test]
fn a_live_listener_is_answered() {
    let temp = TempDir::new().unwrap();
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::unix_socket_bindable(temp.path()),
        "daemon::rpc::tests::a_live_listener_is_answered",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    let listener = std::os::unix::net::UnixListener::bind(socket_path(temp.path())).unwrap();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request: Request = read_message(&mut stream).unwrap();
        write_message(&mut stream, &Response::Pong).unwrap();
    });

    match try_send_request(temp.path(), &ping()).unwrap() {
        DaemonReach::Answered(Response::Pong) => {}
        DaemonReach::Answered(other) => panic!("expected Pong, got {other:?}"),
        DaemonReach::NotListening => panic!("expected Answered, got NotListening"),
        DaemonReach::Unreachable => panic!("expected Answered, got Unreachable"),
    }

    handle.join().unwrap();
}

#[test]
fn a_work_dir_symlink_past_the_socket_path_limit_still_reaches_the_daemon() {
    let temp = TempDir::new().unwrap();
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::unix_socket_bindable(temp.path()),
        "daemon::rpc::tests::a_work_dir_symlink_past_the_socket_path_limit_still_reaches_the_daemon",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    let real = temp.path().join("w");
    std::fs::create_dir(&real).unwrap();
    let link = temp.path().join("a".repeat(100));
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(link.join("orchestrator.sock").as_os_str().len() > 104);
    let listener = std::os::unix::net::UnixListener::bind(socket_path(&real)).unwrap();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request: Request = read_message(&mut stream).unwrap();
        write_message(&mut stream, &Response::Pong).unwrap();
    });

    match try_send_request(&link, &ping()).unwrap() {
        DaemonReach::Answered(Response::Pong) => {}
        DaemonReach::Answered(other) => panic!("expected Pong, got {other:?}"),
        DaemonReach::NotListening => panic!("expected Answered, got NotListening"),
        DaemonReach::Unreachable => panic!("expected Answered, got Unreachable"),
    }

    handle.join().unwrap();
}

#[test]
fn a_socket_path_too_long_even_resolved_is_unreachable() {
    let temp = TempDir::new().unwrap();
    let long = temp.path().join("b".repeat(120));
    std::fs::create_dir(&long).unwrap();
    std::fs::write(long.join("orchestrator.sock"), b"").unwrap();

    match try_send_request(&long, &ping()).unwrap() {
        DaemonReach::Unreachable => {}
        DaemonReach::NotListening => panic!("expected Unreachable, got NotListening"),
        DaemonReach::Answered(response) => panic!("expected Unreachable, got {response:?}"),
    }
}

#[test]
fn a_worktree_spelling_past_sun_path_is_answered() {
    let temp = TempDir::new().unwrap();
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::unix_socket_bindable(temp.path()),
        "daemon::rpc::tests::a_worktree_spelling_past_sun_path_is_answered",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    let real_work = temp.path().join("r/.loom/work");
    std::fs::create_dir_all(&real_work).unwrap();
    let worktree_state = temp
        .path()
        .join("r/.worktrees")
        .join("a".repeat(80))
        .join(".loom");
    std::fs::create_dir_all(&worktree_state).unwrap();
    let spelling = worktree_state.join("work");
    std::os::unix::fs::symlink("../../../.loom/work", &spelling).unwrap();
    assert!(spelling.join(SOCKET_FILE).as_os_str().len() >= 108);
    let listener = std::os::unix::net::UnixListener::bind(socket_path(&real_work)).unwrap();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request: Request = read_message(&mut stream).unwrap();
        write_message(&mut stream, &Response::Pong).unwrap();
    });

    match try_send_request(&spelling, &ping()).unwrap() {
        DaemonReach::Answered(Response::Pong) => {}
        DaemonReach::Answered(other) => panic!("expected Pong, got {other:?}"),
        DaemonReach::NotListening => panic!("expected Answered, got NotListening"),
        DaemonReach::Unreachable => panic!("expected Answered, got Unreachable"),
    }

    handle.join().unwrap();
}
