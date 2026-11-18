//! State cleanup and the overview viewer's socket.

use super::*;
use crate::orchestrator::terminal::tmux::socket::tests::{server_socket_dir, TmuxTmpDirGuard};
use crate::orchestrator::terminal::tmux::socket_path_for;
use crate::orchestrator::terminal::tmux::viewer::viewer_socket_name;
use tempfile::TempDir;

/// `loom clean --all` proceeds past the viewer socket a dead tmux server left.
#[test]
#[serial_test::serial]
fn state_cleanup_proceeds_past_a_dead_viewer_socket() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let socket = socket_path_for(&viewer_socket_name(&work_dir));
    fs::create_dir_all(socket.parent().unwrap()).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
    let mut stats = CleanStats::default();

    retire_viewer_and_clean_state(temp_dir.path(), &mut stats).unwrap();

    assert!(fs::symlink_metadata(&socket).is_err());
    assert!(!work_dir.exists());
    assert!(stats.state_removed);
}

/// A socket this user owns but a live server holds stops the cleanup with the
/// state intact: its session cannot be verified as this repository's viewer.
#[test]
#[serial_test::serial]
fn state_cleanup_stops_at_a_listening_socket_it_cannot_verify() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let socket = socket_path_for(&viewer_socket_name(&work_dir));
    fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let mut stats = CleanStats::default();

    assert!(retire_viewer_and_clean_state(temp_dir.path(), &mut stats).is_err());

    assert!(socket.exists());
    assert!(work_dir.exists());
}
