//! E2E tests for the tmux terminal backend.
//!
//! # Sandbox isolation
//!
//! Verified: inside a Claude Code sandbox, `tmux new-session -d` cannot
//! create a socket under the default directory (`error creating
//! /private/tmp/tmux-<uid>/<name> (Operation not permitted)`) — unsandboxed,
//! the same command succeeds. loom deliberately never emits plan
//! `allow_write` into `sandbox.filesystem.allowWrite` (see the "Do NOT emit
//! allowWrite" comment in `src/sandbox/settings.rs`), so no plan config can
//! grant that write. Both tests here therefore redirect `TMUX_TMPDIR` to a
//! throwaway directory under `/tmp` for their duration, and are `#[serial]`
//! because they mutate process-global env state.
//!
//! `/tmp` (not [`std::env::temp_dir`]) deliberately, for the same reason
//! `loom_socket_dir()` in `src/orchestrator/terminal/tmux/socket.rs` avoids
//! it: on macOS, `std::env::temp_dir()` resolves to a long per-process
//! `$TMPDIR` under `/var/folders/...`. Once tmux appends its own
//! `tmux-<uid>/loom-<session-id>` beneath that, the full socket path can
//! exceed the 104-byte `AF_UNIX sun_path` limit — an environment-specific
//! path-length failure that has nothing to do with the code under test.

use loom::models::session::Session;
use loom::orchestrator::terminal::native::create_wrapper_script;
use loom::orchestrator::terminal::tmux::{
    await_tmux_session_pid, socket_name, socket_path_for, spawn_in_tmux, TmuxBackend,
};
use serial_test::serial;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Redirects `TMUX_TMPDIR` to an isolated per-test directory for the
/// duration of the guard, restoring the previous value and removing the
/// directory on drop — on EVERY exit path, including a panic mid-test.
struct TmuxTmpDirGuard {
    original: Option<std::ffi::OsString>,
    dir: PathBuf,
}

impl TmuxTmpDirGuard {
    fn new() -> Self {
        let dir = PathBuf::from("/tmp").join(format!("loom-e2e-tmux-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create isolated TMUX_TMPDIR");
        let original = std::env::var_os("TMUX_TMPDIR");
        std::env::set_var("TMUX_TMPDIR", &dir);
        Self { original, dir }
    }
}

impl Drop for TmuxTmpDirGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("TMUX_TMPDIR", value),
            None => std::env::remove_var("TMUX_TMPDIR"),
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// CASE 1 (happy path): spawn a wrapper under tmux, discover its PID, and
/// tear it down through `TmuxBackend::kill_session`.
#[test]
#[serial]
fn tmux_spawn_lifecycle_reaches_a_live_pid_and_teardown_clears_it() {
    let _tmux_tmpdir = TmuxTmpDirGuard::new();

    let work_dir = TempDir::new().unwrap();
    let work_dir_path = work_dir.path();

    let mut session = Session::new();
    let socket = socket_name(&session);
    let pid_key = format!("e2e-tmux-{}", session.id);

    // create_wrapper_script `exec`s an arbitrary command, so the PID it
    // records is `sleep`'s, not a shell's.
    let wrapper_path = create_wrapper_script(
        work_dir_path,
        &pid_key,
        "e2e-tmux-stage",
        &session.id,
        "sleep 30",
        Some(work_dir_path),
    )
    .expect("wrapper script should be created");

    let tmux_session_name = "e2e-tmux-session";
    spawn_in_tmux(&socket, tmux_session_name, work_dir_path, &wrapper_path)
        .expect("tmux new-session should succeed under the isolated TMUX_TMPDIR");

    let socket_path = socket_path_for(&socket);
    assert!(
        socket_path.exists(),
        "tmux socket file should exist after a successful spawn"
    );

    let pid = await_tmux_session_pid(work_dir_path, &pid_key, work_dir_path, &session.id)
        .expect("await_tmux_session_pid should discover the sleep PID");
    assert!(
        loom::process::is_process_alive(pid),
        "discovered pid {pid} should be alive"
    );

    session.set_pid(pid);
    let backend = TmuxBackend::new(work_dir_path.to_path_buf());
    backend
        .kill_session(&session)
        .expect("kill_session should tear down the tmux server and the process");

    assert!(
        wait_until(|| !socket_path.exists(), Duration::from_secs(5)),
        "tmux socket file should be gone after kill_session"
    );
    assert!(
        wait_until(
            || !loom::process::is_process_alive(pid),
            Duration::from_secs(5)
        ),
        "sleep process {pid} should be dead after kill_session"
    );
}

/// CASE 2 (pins the silent-failure fix): when the tmux socket directory
/// cannot be created, tmux itself exits 0 (see module docs) — `spawn_in_tmux`
/// must still report `Err`, not `Ok`.
#[test]
#[serial]
fn spawn_in_tmux_errors_when_socket_dir_is_unwritable() {
    use std::os::unix::fs::PermissionsExt;

    let unwritable_parent = TempDir::new().unwrap();

    // Strip write permission on the parent so tmux cannot create its
    // `tmux-<uid>` socket subdirectory inside it — the exact condition
    // verified to make tmux print "error creating <path> (Operation not
    // permitted)" on stderr while still exiting 0.
    let mut perms = std::fs::metadata(unwritable_parent.path())
        .unwrap()
        .permissions();
    perms.set_mode(0o500); // read + execute, no write
    std::fs::set_permissions(unwritable_parent.path(), perms).unwrap();

    let original = std::env::var_os("TMUX_TMPDIR");
    std::env::set_var("TMUX_TMPDIR", unwritable_parent.path());

    let work_dir = TempDir::new().unwrap();
    let session = Session::new();
    let socket = socket_name(&session);
    let pid_key = format!("e2e-tmux-fail-{}", session.id);

    let result = create_wrapper_script(
        work_dir.path(),
        &pid_key,
        "e2e-tmux-fail-stage",
        &session.id,
        "sleep 30",
        Some(work_dir.path()),
    )
    .and_then(|wrapper| spawn_in_tmux(&socket, "e2e-tmux-fail-session", work_dir.path(), &wrapper));

    // Restore env and permissions (so the TempDir can clean itself up)
    // before asserting, on this and every future exit path from here.
    match &original {
        Some(value) => std::env::set_var("TMUX_TMPDIR", value),
        None => std::env::remove_var("TMUX_TMPDIR"),
    }
    let mut restore_perms = std::fs::metadata(unwritable_parent.path())
        .unwrap()
        .permissions();
    restore_perms.set_mode(0o700);
    let _ = std::fs::set_permissions(unwritable_parent.path(), restore_perms);

    assert!(
        result.is_err(),
        "spawn_in_tmux must return Err when the tmux socket directory cannot be created, \
         even though tmux itself exits 0 in this scenario"
    );
}
