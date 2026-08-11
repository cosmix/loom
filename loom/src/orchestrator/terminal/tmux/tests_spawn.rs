//! Spawn-abort tests for `tmux/mod.rs`, split from `tests.rs` to keep both
//! modules under the 400-line ceiling (CLAUDE.md Rule 17).

use super::socket::tests::TmuxTmpDirGuard;
use super::*;
use crate::models::session::SessionType;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use serial_test::serial;
use std::path::PathBuf;

/// Stands in for a wrapper script an attempted spawn already wrote, so the
/// abort path has something to clean up.
fn wrapper_left_by_an_attempted_spawn(work: &Path, pid_key: &str) {
    native::create_wrapper_script(
        work,
        pid_key,
        "abort-stage",
        "session-abcd1234-1111111111",
        "claude 'prompt'",
        None,
        SessionType::Stage,
    )
    .unwrap();
}

#[test]
#[serial]
fn aborting_a_spawn_removes_the_pid_file_and_wrapper_it_created() {
    // `abort_tmux_spawn` resolves the socket through `socket_path_for`, which
    // reads the process-global `$TMUX_TMPDIR`, then `remove_file`s the result.
    // Two hazards, one guard each: `#[serial]`, because socket.rs's
    // `TmuxTmpDirGuard` tests rewrite that var and `#[serial]` only serialises
    // against OTHER `#[serial]` tests; and the pin, because an ambient value
    // (or the `/tmp` default) aims that removal at `tmux-<uid>`, where every
    // loom checkout on this box keeps its live session sockets.
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());

    // THE REGRESSION THIS PINS: without this cleanup, a tmux spawn that failed
    // AFTER the wrapper ran leaves `.work/pids/<pid_key>.pid` behind. The
    // native retry reuses the same `Session`, so `prepare_session_launch`
    // derives the SAME pid_key, `create_wrapper_script` does not truncate the
    // file, and `await_session_pid` returns the first LIVE pid it reads there
    // — the orphaned tmux claude — while stamping `backend = Native`.
    //
    // No tmux server is needed: `kill_socket_server` against an absent socket
    // is a harmless no-op, so the file half of the teardown is what is under
    // test here. (The server half is covered end-to-end in tests/e2e.)
    let work = tempfile::TempDir::new().unwrap();
    let pid_key = "loom-abort-stage-session-abcd1234-1111111111";
    wrapper_left_by_an_attempted_spawn(work.path(), pid_key);
    let pid_file = work.path().join("pids").join(format!("{pid_key}.pid"));
    std::fs::write(&pid_file, "424242\n").unwrap();

    abort_tmux_spawn(work.path(), "loom-session-abcd1234-1111111111", pid_key);

    assert!(
        !pid_file.exists(),
        "the stale PID file must not survive for the native retry to adopt"
    );
    assert!(
        !work
            .path()
            .join("wrappers")
            .join(format!("{pid_key}-wrapper.sh"))
            .exists(),
        "the wrapper script of the abandoned attempt must be removed too"
    );
}

/// Strips write permission from a directory so tmux cannot create its
/// `tmux-<uid>` socket directory inside it — the condition
/// `tests/e2e/tmux_backend.rs` case 2 verified makes tmux exit 1 with
/// `couldn't create directory ... (Permission denied)`. Mirrors that file's
/// own permission guard; the two cannot be shared across the lib/integration
/// crate boundary without exporting a test-only item from the library.
///
/// Restores the original mode on drop, on EVERY exit path including a
/// panicking assertion: a directory left at 0o500 cannot be emptied, so its
/// `TempDir` would fail to clean itself up and leak into `$TMPDIR`.
struct UnwritableDirGuard {
    dir: PathBuf,
    original: std::fs::Permissions,
}

impl UnwritableDirGuard {
    fn strip_write(dir: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let original = std::fs::metadata(dir)
            .expect("the directory to make unwritable must exist")
            .permissions();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500))
            .expect("stripping write permission must succeed");
        Self {
            dir: dir.to_path_buf(),
            original,
        }
    }
}

impl Drop for UnwritableDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.dir, self.original.clone());
    }
}

/// Puts an immediately-exiting `claude` stub at the FRONT of `$PATH` for the
/// duration of the guard, and restores `$PATH` on drop — including on a panic.
///
/// Two jobs, and the second is why it prepends rather than appends:
/// 1. `prepare_session_launch` calls `find_claude_path()`, which fails outright
///    on a host with no claude — before any wrapper or PID file exists, so the
///    test below would go red over a missing binary rather than over the abort
///    wiring it exists to pin.
/// 2. Where tmux CAN create its socket anyway (root writes into a 0o500
///    directory), the wrapper runs and `exec`s whatever `claude` resolved to.
///    A real claude there would launch an unsupervised agent from a unit test;
///    the stub exits instead, so the spawn still fails and still aborts.
///
/// The stub is never executed on the ordinary path — the caller also pins
/// `[remote_control] mode = "off"`, so the preflight `claude --version` (whose
/// verdict is memoized process-wide) never fires either.
struct ClaudeOnPathGuard {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl ClaudeOnPathGuard {
    fn install() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let stub = dir.path().join("claude");
        std::fs::write(&stub, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let original = std::env::var_os("PATH");
        let mut entries = vec![dir.path().to_path_buf()];
        if let Some(path) = original.as_ref() {
            entries.extend(std::env::split_paths(path));
        }
        std::env::set_var("PATH", std::env::join_paths(entries).unwrap());

        Self {
            _dir: dir,
            original,
        }
    }
}

impl Drop for ClaudeOnPathGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

#[test]
#[serial]
fn a_failed_spawn_aborts_and_leaves_no_pid_file_for_the_native_retry_to_adopt() {
    // THE WIRING THIS PINS: `abort_tmux_spawn` was tested in ISOLATION only,
    // so nothing asserted that `TmuxBackend::spawn` actually CALLS it. Deleting
    // `abort();` from an error arm left the whole suite green while
    // reintroducing the two-claude-agents-in-one-worktree bug the abort exists
    // to prevent: `dispatch_spawn` retries on the native lane, which reuses
    // this same `Session` and therefore derives the same pid_key, so a
    // surviving PID file makes `await_session_pid` adopt the orphaned tmux
    // claude while stamping `backend = Native`.
    //
    // This normally drives the FIRST of `spawn`'s three abort arms, the
    // `spawn_in_tmux` failure. A runner that can write into a 0o500 directory
    // anyway (root) gets a server, then a wrapper that `exec`s the
    // `ClaudeOnPathGuard` stub and exits, and so lands on the SECOND arm —
    // `await_tmux_session_pid` finding no live PID. Either arm proves the same
    // wiring, and the positive control below records which one ran. The third,
    // `try_mark_running` failing, needs a session already past `Spawning` and
    // is not reachable from here.
    let work = tempfile::TempDir::new().unwrap();
    // `remote_control::resolve` otherwise runs `claude --version` and memoizes
    // the verdict for the whole test PROCESS — this test must neither execute
    // a binary nor decide that cached answer for every other test.
    crate::fs::work_dir::write_remote_control_config(
        work.path(),
        &crate::remote_control::RemoteControlConfig {
            mode: crate::remote_control::RemoteControlMode::Off,
        },
    )
    .unwrap();
    let _claude = ClaudeOnPathGuard::install();

    // Declared in this order so drop runs in reverse: `TMUX_TMPDIR` is
    // restored first, then the directory's write permission, and only then
    // does `socket_dir` try to delete itself — a directory still at 0o500
    // could not be emptied.
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _unwritable = UnwritableDirGuard::strip_write(socket_dir.path());
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());

    let stage = Stage::new("tmux-abort-wiring".to_string(), None);
    let mut session = Session::new();
    session.assign_to_stage(stage.id.clone());
    // The pid_key `prepare_session_launch` will derive for this session
    // (`tracking_key` + `session.id`). `spawn` re-assigns the same stage with
    // the same `SessionType::Stage`, which is idempotent, so the key it
    // computes internally is the one derived here.
    let (_, pid_key) = native::NativeBackend::window_title_and_pid_key(&session).unwrap();

    // Stands in for a wrapper that already ran and recorded its PID before the
    // spawn failed. NOTHING but `abort_tmux_spawn` deletes this file — the
    // spawn path never writes it and never truncates it — so its absence
    // afterwards is precisely the proof that `spawn` reached its abort path.
    std::fs::create_dir_all(work.path().join("pids")).unwrap();
    let pid_file = work.path().join("pids").join(format!("{pid_key}.pid"));
    std::fs::write(&pid_file, "424242\n").unwrap();

    let worktree = Worktree::new(
        stage.id.clone(),
        work.path().to_path_buf(),
        format!("loom/{}", stage.id),
    );
    let socket = socket_name(&session);
    let backend = TmuxBackend::new(work.path().to_path_buf());
    let err = backend
        .spawn_session(&stage, &worktree, session, &work.path().join("signal.md"))
        .expect_err(
            "the spawn must fail: tmux cannot create its socket directory under a mode-0500 \
             TMUX_TMPDIR, and where it can (root) the claude stub exits immediately, so no \
             live PID is ever discovered",
        );

    // POSITIVE CONTROL. Both assertions below would also hold if the spawn had
    // died inside `prepare_session_launch` — before any abort was owed. These
    // are the two post-preparation shapes: every `spawn_in_tmux` error names
    // the socket, and `await_session_pid`'s names the pid_key it gave up on.
    // Neither string can come out of `prepare_session_launch`, whose failures
    // report a missing claude binary or an unwritable wrapper path.
    let message = err.to_string();
    assert!(
        message.contains(&socket) || message.contains("No PID discovered"),
        "the spawn must have failed at or after the tmux step, not before it: {err}"
    );

    assert!(
        !pid_file.exists(),
        "the failed spawn must delete the PID file under its own pid_key, or the native \
         retry adopts the tmux attempt's process (spawn error was: {err})"
    );
    assert!(
        !work
            .path()
            .join("wrappers")
            .join(format!("{pid_key}-wrapper.sh"))
            .exists(),
        "the wrapper script this spawn created must be removed on the failure path too"
    );
}
