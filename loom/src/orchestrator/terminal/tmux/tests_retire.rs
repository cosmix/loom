use super::retire::{ensure_same_socket, has_no_listener};
use super::*;
use crate::orchestrator::terminal::tmux::socket::tests::{
    server_socket_dir, TmuxServerGuard, TmuxTmpDirGuard,
};
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

fn start_server(name: &str, session: &str) {
    let output = super::super::run_tmux_control(
        &["-L", name, "new-session", "-d", "-s", session],
        super::super::TMUX_SPAWN_TIMEOUT,
        "start viewer retirement test server",
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[serial]
fn retirement_only_targets_this_repo_viewer() {
    if which::which("tmux").is_err() {
        return;
    }
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();
    let work_a = repo_a.path().join(".loom/work");
    let work_b = repo_b.path().join(".loom/work");
    fs::create_dir_all(&work_a).unwrap();
    fs::create_dir_all(&work_b).unwrap();
    let a = viewer_socket_name(&work_a);
    let b = viewer_socket_name(&work_b);
    let _a_guard = TmuxServerGuard::new(&a);
    let _b_guard = TmuxServerGuard::new(&b);
    let _other_guard = TmuxServerGuard::new("unrelated-viewer");
    start_server(&a, OVERVIEW_SESSION);
    start_server(&b, OVERVIEW_SESSION);
    start_server("unrelated-viewer", "other-session");

    assert!(retire_viewer(&work_a).unwrap());
    assert!(!socket_path_for(&a).exists());
    assert!(socket_path_for(&b).exists());
    assert!(socket_path_for("unrelated-viewer").exists());
    start_server(&a, "wrong-session");
    assert!(retire_viewer(&work_a).is_err());
    assert!(socket_path_for(&a).exists());
}

#[test]
#[serial]
fn retirement_rejects_non_socket_at_expected_name() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = repo.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "foreign data").unwrap();

    assert!(retire_viewer(&work_dir).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"foreign data");
}

/// The socket file a tmux server leaves when it dies without unlinking it: the
/// file stays and nothing listens on it. Its directory has the mode tmux requires.
fn leave_dead_socket(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).unwrap();
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
    drop(std::os::unix::net::UnixListener::bind(path).unwrap());
}

#[test]
#[serial]
fn retirement_removes_the_owned_socket_of_a_dead_server() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = repo.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    leave_dead_socket(&path);

    assert!(retire_viewer(&work_dir).unwrap());

    assert!(fs::symlink_metadata(&path).is_err(), "the socket is gone");
    assert!(
        !retire_viewer(&work_dir).unwrap(),
        "nothing is left to retire"
    );
}

#[test]
#[serial]
fn retirement_refuses_a_listening_socket_it_cannot_verify() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = repo.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();

    assert!(retire_viewer(&work_dir).is_err());

    assert!(path.exists(), "a live listener's socket stays");
}

#[test]
#[serial]
fn retirement_refuses_a_symlink_at_the_expected_name() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = repo.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    let elsewhere = socket_root.path().join("elsewhere");
    leave_dead_socket(&elsewhere);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();

    assert!(retire_viewer(&work_dir).is_err());

    assert!(fs::symlink_metadata(&path).is_ok());
    assert!(fs::symlink_metadata(&elsewhere).is_ok());
}

/// A socket a server has bound and not yet listened on: connecting is refused, as
/// for a dead server. Hold the descriptor to keep it bound.
fn bind_without_listening(path: &Path) -> std::os::fd::OwnedFd {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // SAFETY: an all-zero sockaddr_un is valid; socket and bind get valid arguments,
    // and the descriptor is owned from the moment it is checked.
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        assert!(fd >= 0);
        let owned = OwnedFd::from_raw_fd(fd);
        let mut address: libc::sockaddr_un = std::mem::zeroed();
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let bytes = path.as_os_str().as_bytes();
        assert!(bytes.len() < address.sun_path.len());
        for (slot, byte) in address.sun_path.iter_mut().zip(bytes) {
            *slot = *byte as libc::c_char;
        }
        let length = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
        let bound = libc::bind(fd, (&raw const address).cast(), length);
        assert_eq!(bound, 0, "{}", std::io::Error::last_os_error());
        owned
    }
}

fn lock_path(socket: &Path) -> std::path::PathBuf {
    let mut path = socket.as_os_str().to_owned();
    path.push(".lock");
    path.into()
}

/// What a tmux client holds from before it unlinks the old socket until its server
/// listens.
fn hold_start_lock(socket: &Path) -> fs::File {
    use std::os::fd::AsRawFd;
    let file = fs::File::create(lock_path(socket)).unwrap();
    // SAFETY: `file` is an open descriptor for the duration of the call.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    assert_eq!(locked, 0);
    file
}

fn work_dir_in(repo: &TempDir) -> std::path::PathBuf {
    let work_dir = repo.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    work_dir
}

#[test]
#[serial]
fn retirement_leaves_the_socket_of_a_server_about_to_listen() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = work_dir_in(&repo);
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    let _bound = bind_without_listening(&path);
    let _lock = hold_start_lock(&path);
    assert!(has_no_listener(&path), "it refuses as a dead server does");

    let error = retire_viewer(&work_dir).unwrap_err().to_string();

    assert!(error.contains("starting"), "{error}");
    assert!(
        fs::symlink_metadata(&path).is_ok(),
        "the starting server's socket stays"
    );
    assert!(lock_path(&path).exists(), "the holder's lock file stays");
}

#[test]
#[serial]
fn retirement_removes_an_unheld_lock_file_with_the_dead_socket() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = work_dir_in(&repo);
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    leave_dead_socket(&path);
    fs::write(lock_path(&path), "").unwrap();

    assert!(retire_viewer(&work_dir).unwrap());

    assert!(fs::symlink_metadata(&path).is_err());
    assert!(fs::symlink_metadata(lock_path(&path)).is_err());
}

#[test]
#[serial]
fn retirement_does_not_follow_a_symlink_at_the_lock_path() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = work_dir_in(&repo);
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    leave_dead_socket(&path);
    let elsewhere = socket_root.path().join("elsewhere");
    std::os::unix::fs::symlink(&elsewhere, lock_path(&path)).unwrap();

    assert!(retire_viewer(&work_dir).is_err());

    assert!(!elsewhere.exists());
    assert!(fs::symlink_metadata(&path).is_ok());
}

/// The lock is worth holding only if tmux on this host honours it: a client stays
/// away from the dead socket while the lock is held, and starts its server once it
/// is released.
#[test]
#[serial]
fn a_real_tmux_client_waits_for_the_start_lock() {
    if which::which("tmux").is_err() {
        return;
    }
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let name = "lock-honoured";
    let _server = TmuxServerGuard::new(name);
    let path = socket_path_for(name);
    leave_dead_socket(&path);
    let dead = fs::symlink_metadata(&path).unwrap();
    let lock = start_lock::StartLock::try_take(&path).unwrap().unwrap();

    let client = std::thread::spawn(move || start_server(name, "other-session"));
    std::thread::sleep(std::time::Duration::from_millis(500));

    assert!(
        !client.is_finished(),
        "the client did not wait for the lock"
    );
    ensure_same_socket(&path, &dead).unwrap();
    drop(lock);
    client.join().unwrap();
    assert!(!has_no_listener(&path), "the client started its server");
}

/// Retirement racing a real start. Every server the race starts must stay
/// reachable: the only socket retirement may unlink is one nothing will listen on.
#[test]
#[serial]
fn retirement_racing_a_real_start_keeps_the_started_server() {
    if which::which("tmux").is_err() {
        return;
    }
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = work_dir_in(&repo);
    let name = viewer_socket_name(&work_dir);
    let _server = TmuxServerGuard::new(&name);
    let path = socket_path_for(&name);
    for round in 0..20 {
        leave_dead_socket(&path);
        let starting = name.clone();
        let client = std::thread::spawn(move || start_server(&starting, "other-session"));
        // Either outcome is right: the socket was dead, or a server holds it.
        let _ = retire_viewer(&work_dir);
        client.join().unwrap();

        let listed = super::super::run_tmux_control(
            &["-L", &name, "has-session", "-t", "other-session"],
            super::super::TMUX_PROBE_TIMEOUT,
            "probe the raced server",
        )
        .unwrap();
        assert!(
            listed.status.success(),
            "round {round}: the started server lost its socket"
        );
        super::super::teardown_socket(&name).unwrap();
        let _ = fs::remove_file(&path);
    }
}
