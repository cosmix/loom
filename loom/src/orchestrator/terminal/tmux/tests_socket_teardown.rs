use super::socket::tests::{server_socket_dir, TmuxTmpDirGuard};
use super::*;
use crate::process::sandbox_probe::require_unix_socket;
use serial_test::serial;
use std::os::unix::net::UnixListener;

#[test]
#[serial]
fn teardown_removes_a_socket_nothing_listens_on() {
    let socket_dir = server_socket_dir();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    let socket = "loom-stale-teardown";
    let path = socket_path_for(socket);
    require_unix_socket!("teardown_removes_a_socket_nothing_listens_on", &path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let listener = UnixListener::bind(&path).unwrap();
    drop(listener);

    teardown_socket(socket).unwrap();

    assert!(!path.exists());
}

#[test]
#[serial]
fn teardown_keeps_a_socket_that_still_has_a_listener() {
    let socket_dir = server_socket_dir();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    let socket = "loom-listening-teardown";
    let path = socket_path_for(socket);
    require_unix_socket!("teardown_keeps_a_socket_that_still_has_a_listener", &path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _listener = UnixListener::bind(&path).unwrap();

    let error = teardown_socket(socket).unwrap_err();

    assert!(error.to_string().contains("tmux kill-server failed"));
    assert!(path.exists());
}
