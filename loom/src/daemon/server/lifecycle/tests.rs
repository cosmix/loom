//! Boundary tests for the `sun_path` length check ahead of the daemon's
//! socket bind (see `crate::daemon::socket`). Exercised at 103/104/105 bytes
//! rather than a realistic path, since every realistic loom socket path
//! passes and would prove nothing about the boundary itself. Then the ready
//! byte `run_server` writes once the bind succeeds.

use super::{socket_path_fits, DaemonServer, READY_BYTE, SUN_PATH_MAX};
use crate::daemon::DaemonConfig;
use crate::process::sandbox_probe::{skip_unless, unix_socket_bindable};
use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

fn path_of_len(len: usize) -> PathBuf {
    PathBuf::from("a".repeat(len))
}

#[test]
fn fits_one_byte_under_the_limit() {
    let path = path_of_len(SUN_PATH_MAX - 1);
    assert_eq!(path.as_os_str().len(), 103);
    assert!(socket_path_fits(&path));
}

#[test]
fn rejects_exactly_at_the_limit() {
    // At exactly SUN_PATH_MAX bytes there is no room left for the kernel's
    // NUL terminator, so this must NOT fit.
    let path = path_of_len(SUN_PATH_MAX);
    assert_eq!(path.as_os_str().len(), 104);
    assert!(!socket_path_fits(&path));
}

#[test]
fn rejects_one_byte_over_the_limit() {
    let path = path_of_len(SUN_PATH_MAX + 1);
    assert_eq!(path.as_os_str().len(), 105);
    assert!(!socket_path_fits(&path));
}

/// `run_server` writes the ready byte once its socket is bound, then closes
/// the readiness pipe. The shutdown flag is set first, so the accept loop and
/// every background thread stop at once: a live quota poller would read this
/// host's credentials and poll the provider.
#[test]
fn run_server_writes_the_ready_byte_once_its_socket_is_bound() {
    let dir = TempDir::new().expect("temp dir");
    if skip_unless(
        unix_socket_bindable(dir.path()),
        "daemon::server::lifecycle::tests::run_server_writes_the_ready_byte_once_its_socket_is_bound",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    let server = DaemonServer::with_config(dir.path(), DaemonConfig::default());
    let lock = server.acquire_exclusive_lock().expect("singleton lock");
    let (mut reader, writer) = std::io::pipe().expect("readiness pipe");
    server.shutdown_flag.store(true, Ordering::SeqCst);

    server
        .run_server(lock, OwnedFd::from(writer))
        .expect("run_server binds, reports ready and stops");

    let mut written = Vec::new();
    reader
        .read_to_end(&mut written)
        .expect("read the readiness pipe");
    assert_eq!(written, [READY_BYTE], "one ready byte, then EOF");
    assert!(
        server.was_running.load(Ordering::SeqCst),
        "the bind marked the server live before the ready byte"
    );
}

/// The ready byte follows the bind, never precedes it: a directory planted at
/// the socket path fails the bind on every host, sandboxed or not, and the
/// pipe closes empty.
#[test]
fn a_failed_bind_writes_no_ready_byte() {
    let dir = TempDir::new().expect("temp dir");
    let server = DaemonServer::with_config(dir.path(), DaemonConfig::default());
    std::fs::create_dir(&server.socket_path).expect("plant a directory at the socket path");
    let lock = server.acquire_exclusive_lock().expect("singleton lock");
    let (mut reader, writer) = std::io::pipe().expect("readiness pipe");

    let result = server.run_server(lock, OwnedFd::from(writer));

    let mut written = Vec::new();
    reader
        .read_to_end(&mut written)
        .expect("read the readiness pipe");
    assert!(result.is_err(), "a bind over a directory fails");
    assert!(
        written.is_empty(),
        "no ready byte without a bound socket: {written:?}"
    );
}
