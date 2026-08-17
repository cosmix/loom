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
//! grant that write. Every test here therefore redirects `TMUX_TMPDIR` to a
//! throwaway directory under `/tmp` for its duration, and is `#[serial]`
//! because they mutate process-global env state.
//!
//! `/tmp` is preferred over [`std::env::temp_dir`], for the same reason
//! `loom_socket_dir()` in `src/orchestrator/terminal/tmux/socket.rs` prefers
//! it: on macOS, `std::env::temp_dir()` resolves to a long per-process
//! `$TMPDIR` under `/var/folders/...`. Once tmux appends its own
//! `tmux-<uid>/loom-<session-id>` beneath that, the full socket path can
//! exceed the 104-byte `AF_UNIX sun_path` limit — an environment-specific
//! path-length failure that has nothing to do with the code under test.
//! `TmuxTmpDirGuard` therefore only falls back to `std::env::temp_dir()`
//! when `/tmp` itself turns out not to be writable (e.g. inside a sandbox
//! that mounts it read-only), and even then only after checking that the
//! projected socket path still fits `sun_path` — see
//! `create_isolated_tmux_tmpdir()`.

use loom::models::session::{Session, SessionType};
use loom::orchestrator::terminal::native::create_wrapper_script;
use loom::orchestrator::terminal::tmux::{
    await_tmux_session_pid, kill_socket_server, socket_name, socket_path_for, spawn_in_tmux,
    TmuxBackend,
};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serial_test::serial;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

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
    )
    .expect("wrapper script creation does not depend on TMUX_TMPDIR and must succeed")
}

/// Restores a process env var to its previous value on drop, on EVERY exit
/// path including a panic -- so overriding a process-global var (like
/// `TMUX_TMPDIR` below) can never leak a stale value into whichever test the
/// harness runs next.
struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

/// The stricter of the two platform `AF_UNIX sun_path` limits (104 bytes on
/// macOS/BSD, 108 on Linux) -- used so a length check passing here holds on
/// every platform this suite runs on.
const SUN_PATH_LIMIT: usize = 104;

/// Bytes reserved for the socket's own final path component,
/// `loom-<session-id>`. Real session ids look like
/// `session-6d4cf60c-1786963954`, so this leaves headroom for the `loom-`
/// prefix plus a realistic id rather than measuring one exactly.
const SOCKET_NAME_BUDGET: usize = 40;

/// Creates and returns the isolated per-test directory to use as
/// `TMUX_TMPDIR`, picking the first candidate base -- `/tmp`, then
/// [`std::env::temp_dir`] -- under which it can actually be created. `/tmp`
/// is preferred for its short path and because it matches tmux's own
/// convention (see the module docs); the fallback exists for exactly the
/// case where `/tmp` is not writable -- e.g. inside a sandbox that mounts it
/// read-only.
///
/// A candidate is usable only if BOTH hold: the per-test directory can
/// actually be created there (rejects a read-only `/tmp`), and the socket
/// path tmux will build beneath it -- `<dir>/tmux-<uid>/loom-<session-id>`,
/// per `loom_socket_dir()` in `src/orchestrator/terminal/tmux/socket.rs` --
/// projects under the `sun_path` limit. Skipping the second check would
/// trade one environment-specific failure (an unwritable `/tmp`) for
/// another, further down the stack in `tmux` itself, where the fallback's
/// long per-process path (e.g. macOS's `/var/folders/...`) can silently
/// blow the socket path budget.
///
/// Panics naming every rejected candidate and why if none qualifies -- a
/// skipped test is a test that can never fail, so this never skips.
fn create_isolated_tmux_tmpdir() -> PathBuf {
    // SAFETY: getuid() is always safe to call and cannot fail. Matches
    // `loom_socket_dir()` in `src/orchestrator/terminal/tmux/socket.rs`,
    // which builds the real socket path the same way.
    let uid = unsafe { libc::getuid() };
    let mut rejected = Vec::new();
    let mut tried = Vec::new();

    for base in [PathBuf::from("/tmp"), std::env::temp_dir()] {
        let dir = base.join(format!("loom-e2e-tmux-{}", std::process::id()));
        // `std::env::temp_dir()` falls back to `/tmp` when `$TMPDIR` is
        // unset, so the two candidates can coincide -- skip a repeat rather
        // than re-trying (and re-reporting) the identical path.
        if tried.contains(&dir) {
            continue;
        }
        tried.push(dir.clone());

        if let Err(err) = std::fs::create_dir_all(&dir) {
            rejected.push(format!("{} (unwritable: {err})", dir.display()));
            continue;
        }

        let projected_len =
            dir.display().to_string().len() + format!("/tmux-{uid}/").len() + SOCKET_NAME_BUDGET;
        if projected_len > SUN_PATH_LIMIT {
            rejected.push(format!(
                "{} (projected socket path {projected_len} bytes exceeds sun_path limit {SUN_PATH_LIMIT})",
                dir.display()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            continue;
        }

        return dir;
    }

    panic!("no usable TMUX_TMPDIR base found; rejected candidates: {rejected:?}");
}

/// Redirects `TMUX_TMPDIR` to an isolated per-test directory for the
/// duration of the guard, additionally removing the directory on drop -- on
/// EVERY exit path, including a panic mid-test.
struct TmuxTmpDirGuard {
    _env: EnvVarGuard,
    dir: PathBuf,
}

impl TmuxTmpDirGuard {
    fn new() -> Self {
        let dir = create_isolated_tmux_tmpdir();
        let _env = EnvVarGuard::set("TMUX_TMPDIR", &dir);
        Self { _env, dir }
    }
}

impl Drop for TmuxTmpDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Tears down a tmux server started for a test on EVERY exit path, including
/// a panicking assertion mid-test -- without this, a failing assertion
/// between spawn and an explicit teardown call would strand a live tmux
/// server outside the test's own `TMUX_TMPDIR` cleanup.
struct TmuxServerGuard {
    socket_path: PathBuf,
}

impl Drop for TmuxServerGuard {
    fn drop(&mut self) {
        kill_socket_server(&self.socket_path);
        let _ = std::fs::remove_file(&self.socket_path);
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

/// CASE 2 (pins the real permission-denied failure mode): when the tmux
/// socket directory's PARENT cannot be written to, tmux exits **1** with
/// `couldn't create directory ... (Permission denied)` on stderr (verified
/// below, and documented at `tmux/mod.rs:74-81`) — `spawn_in_tmux` must
/// propagate that as `Err`, carrying tmux's real stderr text, rather than
/// swallow it.
///
/// This is a DIFFERENT case from the "exit 0 with stderr" rule: that shape
/// needs the socket directory to already exist while only socket creation
/// itself is denied — a sandbox/seccomp condition no CI runner can be relied
/// on to reproduce (see `tmux/mod.rs:74-81`) — so it is pinned instead by the
/// pure `evaluate_new_session` unit tests in
/// `src/orchestrator/terminal/tmux/tests.rs`
/// (`new_session_exit_zero_with_stderr_is_a_failure`).
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

/// CASE 3 (the reason `is_session_alive` is PID-based, not
/// `tmux has-session`-based): a tmux server whose pane process has died but
/// which has not yet reaped itself still answers `has-session` with exit 0.
/// This proves BOTH halves at the same moment — the server really is still
/// up, AND `TmuxBackend::is_session_alive` still reports `false` — because
/// without the first half the second is vacuous: it would pass identically
/// even if `is_session_alive` called `has-session` internally.
///
/// (The unit test of this same name in
/// `src/orchestrator/terminal/tmux/tests.rs` never starts a real tmux
/// server at all, so it cannot pin this property — that is why the
/// canonical version lives here as a real e2e test.)
#[test]
#[serial]
fn tmux_liveness_ignores_running_server_when_pid_is_dead() {
    let _tmux_tmpdir = TmuxTmpDirGuard::new();

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
