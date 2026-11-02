//! E2E tests for the tmux terminal backend.
//!
//! Each test isolates the tmux socket directory and serializes environment changes.
//! Directory writability does not guarantee AF_UNIX bind permission, so real-server
//! tests use a separate bind probe. `LOOM_E2E_REQUIRE_TMUX=1` makes failure fatal.
//! See [tmux backend notes](../../../doc/loom/knowledge/mistakes/tmux-backend.md) for the platform constraint.

use loom::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS;
use loom::models::session::{Session, SessionType};
use loom::orchestrator::terminal::native::create_wrapper_script;
use loom::orchestrator::terminal::tmux::{
    await_tmux_session_pid, socket_name, socket_path_for, spawn_in_tmux, TmuxBackend,
};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serial_test::serial;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

use crate::tmux_guards::EnvVarGuard;
pub(crate) use crate::tmux_guards::{TmuxServerGuard, TmuxTmpDirGuard};

/// A stage-session wrapper script that `exec`s `command` in `dir`.
///
/// `create_wrapper_script` `exec`s an arbitrary command, so the PID it records
/// is that command's, not a shell's — which is what lets these tests stand up
/// a real, killable process without a real claude.
fn stage_wrapper(work_dir: &Path, pid_key: &str, stage_id: &str, session_id: &str) -> PathBuf {
    create_wrapper_script(
        work_dir,
        pid_key,
        stage_id,
        session_id,
        "sleep 30",
        Some(work_dir),
        SessionType::Stage,
        DEFAULT_CONTEXT_CEILING_TOKENS,
    )
    .expect("wrapper script creation does not depend on TMUX_TMPDIR and must succeed")
}

/// Probes whether `dir` can actually host an `AF_UNIX` socket, which is
/// strictly stronger than being writable: a sandbox can permit `mkdir`
/// and still deny `bind(2)`. See
/// [tmux backend notes](../../../doc/loom/knowledge/mistakes/tmux-backend.md) -- under exactly that
/// condition `tmux new-session -d` prints an error and still exits 0.
/// Binds and immediately unlinks a throwaway socket; returns the OS error
/// string on failure.
fn tmux_bind_supported(dir: &Path) -> Result<(), String> {
    // Short name: the whole point of the surrounding sun_path budget is that
    // the final path component stays small, and a probe socket is no
    // exception.
    let probe_path = dir.join("p.sock");
    let _ = std::fs::remove_file(&probe_path);
    let result = std::os::unix::net::UnixListener::bind(&probe_path)
        .map(|_listener| ())
        .map_err(|err| err.to_string());
    let _ = std::fs::remove_file(&probe_path);
    result
}

/// Returns true if the caller should skip: the environment cannot host an
/// `AF_UNIX` socket, so a tmux server cannot start for reasons that have
/// nothing to do with the code under test. Prints a loud `SKIP` line naming
/// the probe error. Panics instead of skipping when
/// `LOOM_E2E_REQUIRE_TMUX=1`, so CI can demand a real run.
pub(crate) fn skip_unless_tmux_can_bind(dir: &Path, test_name: &str) -> bool {
    match tmux_bind_supported(dir) {
        Ok(()) => false,
        Err(err) => {
            if std::env::var("LOOM_E2E_REQUIRE_TMUX").as_deref() == Ok("1") {
                panic!(
                    "{test_name}: this environment cannot bind an AF_UNIX socket under {}: \
                     {err} (LOOM_E2E_REQUIRE_TMUX=1 demands a real run)",
                    dir.display()
                );
            }
            eprintln!(
                "SKIP {test_name}: this environment cannot bind an AF_UNIX socket under {}: \
                 {err} (set LOOM_E2E_REQUIRE_TMUX=1 to fail instead)",
                dir.display()
            );
            true
        }
    }
}

/// Strips write permission from a path and restores the original
/// permissions on drop, on EVERY exit path including a panic -- so making a
/// directory unwritable to force a tmux failure can never outlive the test.
struct PermissionsGuard {
    path: PathBuf,
    original: std::fs::Permissions,
}

impl PermissionsGuard {
    /// Strips write permission (mode `0o500`), capturing the original
    /// permissions to restore on drop.
    fn strip_write(path: &Path) -> Self {
        let original = std::fs::metadata(path)
            .expect("metadata should be readable before permissions are stripped")
            .permissions();
        let mut stripped = original.clone();
        stripped.set_mode(0o500);
        std::fs::set_permissions(path, stripped)
            .expect("write permission should be strippable from an owned temp dir");
        Self {
            path: path.to_path_buf(),
            original,
        }
    }
}

impl Drop for PermissionsGuard {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.path, self.original.clone());
    }
}

pub(crate) fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
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
    if skip_unless_tmux_can_bind(
        _tmux_tmpdir.dir(),
        "tmux_spawn_lifecycle_reaches_a_live_pid_and_teardown_clears_it",
    ) {
        return;
    }

    let work_dir = TempDir::new().unwrap();
    let work_dir_path = work_dir.path();

    let mut session = Session::new();
    // Required so `window_title_and_pid_key` resolves to `Some` inside
    // `kill_session`: without a stage assignment, `tracking_key` stays
    // empty and `kill_session` falls straight back to `session.pid`,
    // skipping the guarded PID-file branch (`pid_matches_entry` /
    // `cleanup_stage_files`) entirely -- the code that exists so loom never
    // SIGTERMs a recycled PID.
    session.assign_to_stage("e2e-tmux-lifecycle-stage".to_string());
    let socket = socket_name(&session);
    // Mirrors production's per-session PID-file key (`tracking_key` +
    // `session.id`, see `launch.rs:55`). `NativeBackend::window_title_and_pid_key`
    // computes this but is `pub(crate)`, unreachable from this external e2e
    // binary, so the exact format is replicated here instead.
    let pid_key = format!("{}-{}", session.tracking_key, session.id);

    let wrapper_path = stage_wrapper(work_dir_path, &pid_key, "e2e-tmux-stage", &session.id);

    let tmux_session_name = "e2e-tmux-session";
    spawn_in_tmux(&socket, tmux_session_name, work_dir_path, &wrapper_path)
        .expect("tmux new-session should succeed under the isolated TMUX_TMPDIR");

    let socket_path = socket_path_for(&socket);
    // Guarantees the server (and its `sleep 30`) are torn down even if one of
    // the ~10 assertions below panics before the explicit `kill_session` call
    // runs. On the passing path this is a harmless no-op: `kill_session` has
    // already removed the socket file by then, leaving nothing for
    // `kill_socket_server` to reach here.
    let _server_guard = TmuxServerGuard {
        socket_path: socket_path.clone(),
    };
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

    // Capture the tmux SERVER's own pid (distinct from the sleep pid above)
    // before teardown. Deleting `kill_socket_server` from
    // `TmuxBackend::kill_session` would still pass the two assertions below
    // it (the socket file is removed by an explicit `remove_file`, and the
    // sleep is killed by `terminate`) while leaving this server process
    // orphaned -- so its death is asserted explicitly.
    let server_pid_output = Command::new("tmux")
        .args(["-L", &socket, "display-message", "-p", "#{pid}"])
        .output()
        .expect("tmux display-message should run");
    assert!(
        server_pid_output.status.success(),
        "tmux display-message failed: {}",
        String::from_utf8_lossy(&server_pid_output.stderr)
    );
    let server_pid: u32 = String::from_utf8_lossy(&server_pid_output.stdout)
        .trim()
        .parse()
        .expect("tmux display-message should print a numeric pid");

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
            || !loom::process::is_process_alive(server_pid),
            Duration::from_secs(5)
        ),
        "tmux server process {server_pid} should be dead after kill_session"
    );
    assert!(
        wait_until(
            || !loom::process::is_process_alive(pid),
            Duration::from_secs(5)
        ),
        "sleep process {pid} should be dead after kill_session"
    );
}

/// A directory permission error propagates tmux stderr; bind denial has separate coverage.
#[test]
#[serial]
fn spawn_in_tmux_errs_when_socket_dir_is_unwritable() {
    let unwritable_parent = TempDir::new().unwrap();

    // Strip write permission on the parent so tmux cannot create its
    // `tmux-<uid>` socket subdirectory inside it — the exact condition
    // verified below to make tmux exit 1 with "couldn't create directory
    // ... (Permission denied)" on stderr. Both guards restore on drop, even
    // on a panic -- unlike the manual restore-before-asserting this replaces.
    let _perms_guard = PermissionsGuard::strip_write(unwritable_parent.path());
    let _tmux_tmpdir = EnvVarGuard::set("TMUX_TMPDIR", unwritable_parent.path());

    let work_dir = TempDir::new().unwrap();
    let session = Session::new();
    let socket = socket_name(&session);
    let pid_key = format!("e2e-tmux-fail-{}", session.id);

    // Wrapper creation is independent of the tmux spawn and must succeed on
    // its own; asserting on a combined `Result` here would let a wrapper
    // failure satisfy `is_err()` without `spawn_in_tmux` ever running.
    let wrapper = stage_wrapper(
        work_dir.path(),
        &pid_key,
        "e2e-tmux-fail-stage",
        &session.id,
    );

    let result = spawn_in_tmux(&socket, "e2e-tmux-fail-session", work_dir.path(), &wrapper);

    let err = result.expect_err(
        "spawn_in_tmux must return Err when the tmux socket directory cannot be created",
    );
    assert!(
        err.to_string().contains("Permission denied"),
        "error must carry tmux's real stderr text, not a generic message: {err}"
    );
}

/// The server remains live after its pane PID dies; liveness must follow the PID.
#[test]
#[serial]
fn tmux_liveness_ignores_running_server_when_pid_is_dead() {
    let _tmux_tmpdir = TmuxTmpDirGuard::new();
    if skip_unless_tmux_can_bind(
        _tmux_tmpdir.dir(),
        "tmux_liveness_ignores_running_server_when_pid_is_dead",
    ) {
        return;
    }

    let work_dir = TempDir::new().unwrap();
    let work_dir_path = work_dir.path();

    let mut session = Session::new();
    // Required so `window_title_and_pid_key` resolves to `Some`, exercising
    // the real PID-FILE layer of `is_session_alive` instead of only the
    // `session.pid` tail.
    session.assign_to_stage("e2e-liveness-stage".to_string());
    let socket = socket_name(&session);
    // Mirrors production's per-session PID-file key (`tracking_key` +
    // `session.id`, see `launch.rs:55`). `NativeBackend::window_title_and_pid_key`
    // computes this but is `pub(crate)`, unreachable from this external e2e
    // binary, so the exact format is replicated here instead.
    let pid_key = format!("{}-{}", session.tracking_key, session.id);

    let wrapper_path = stage_wrapper(work_dir_path, &pid_key, "e2e-liveness-stage", &session.id);

    let tmux_session_name = "e2e-liveness-session";
    spawn_in_tmux(&socket, tmux_session_name, work_dir_path, &wrapper_path)
        .expect("tmux new-session should succeed under the isolated TMUX_TMPDIR");
    // Guarantees the server is torn down even if an assertion below panics.
    let _server_guard = TmuxServerGuard {
        socket_path: socket_path_for(&socket),
    };

    let pid = await_tmux_session_pid(work_dir_path, &pid_key, work_dir_path, &session.id)
        .expect("await_tmux_session_pid should discover the sleep PID");
    session.set_pid(pid);

    let backend = TmuxBackend::new(work_dir_path.to_path_buf());
    assert!(
        backend
            .is_session_alive(&session)
            .expect("is_session_alive should not error"),
        "a freshly spawned, still-running session must read as alive -- \
         otherwise the negative assertion below would prove nothing"
    );

    // Keeps the pane (and therefore the SERVER) alive after its process
    // dies -- the exact condition that would fool a `has-session`-based
    // liveness check.
    let remain_on_exit = Command::new("tmux")
        .args([
            "-L",
            &socket,
            "set-option",
            "-w",
            "-t",
            tmux_session_name,
            "remain-on-exit",
            "on",
        ])
        .output()
        .expect("tmux set-option should run");
    assert!(
        remain_on_exit.status.success(),
        "tmux set-option remain-on-exit failed: {}",
        String::from_utf8_lossy(&remain_on_exit.stderr)
    );

    // SIGKILL, not `loom::process::terminate` (SIGTERM): the point is to
    // simulate an abrupt crash the wrapper/claude process cannot handle.
    let pid_i32 = i32::try_from(pid).expect("sleep pid should fit in i32");
    kill(Pid::from_raw(pid_i32), Signal::SIGKILL)
        .expect("SIGKILL should be deliverable to the sleep process");
    assert!(
        wait_until(
            || !loom::process::is_process_alive(pid),
            Duration::from_secs(5)
        ),
        "sleep process {pid} should be dead after SIGKILL"
    );

    // Verified on tmux 3.7b: with remain-on-exit on, the server answers
    // `has-session` with exit 0 even though the pane's process just died.
    // If this does not hold on some other tmux build, the property this
    // test exists to pin does not hold there either -- the assertion below
    // is what makes that failure visible rather than silently passing.
    let has_session = Command::new("tmux")
        .args(["-L", &socket, "has-session", "-t", tmux_session_name])
        .output()
        .expect("tmux has-session should run");
    assert!(
        has_session.status.success(),
        "tmux server must still report the session alive under remain-on-exit \
         even though its pane process just died -- this is the whole reason \
         is_session_alive must never call has-session (stderr: {})",
        String::from_utf8_lossy(&has_session.stderr)
    );

    assert!(
        !backend
            .is_session_alive(&session)
            .expect("is_session_alive should not error"),
        "is_session_alive must report false from PID evidence alone, even \
         though tmux has-session still reports the server as alive"
    );
}
