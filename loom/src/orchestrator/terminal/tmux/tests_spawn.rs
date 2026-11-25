//! Spawn-abort tests for the tmux backend.

use super::socket::tests::{TmuxServerGuard, TmuxTmpDirGuard};
use super::*;
use crate::models::session::SessionType;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use serial_test::serial;
use std::path::PathBuf;

/// Stands in for a wrapper script an attempted spawn already wrote, so the
/// abort path has something to clean up.
fn wrapper_left_by_an_attempted_spawn(work: &Path, pid_key: &str) {
    launch::create_wrapper_script(
        work,
        pid_key,
        "abort-stage",
        "session-abcd1234-1111111111",
        "claude 'prompt'",
        None,
        SessionType::Stage,
        150_000,
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

    // A failure after the wrapper ran can leave a live tmux process and PID
    // file. The tmux backend reports the failure; cleanup prevents
    // later liveness checks from adopting that abandoned process.
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
        "the stale PID file must not survive an aborted spawn"
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

#[test]
#[serial]
fn failed_teardown_retains_the_only_socket_handle() {
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    let socket = "loom-failed-teardown";
    let path = socket_path_for(socket);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not a tmux socket").unwrap();

    let error = teardown_socket(socket).unwrap_err();

    assert!(format!("{error:#}").contains("tmux kill-server failed"));
    assert!(
        path.exists(),
        "failed teardown must retain its control handle"
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
/// The stub advertises the mandatory capsule flags on `--help` and exits
/// when the wrapper runs it. Remote control is off, so its separate
/// `claude --version` preflight does not run.
struct ClaudeOnPathGuard {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl ClaudeOnPathGuard {
    fn install() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let stub = dir.path().join("claude");
        std::fs::write(&stub, "#!/bin/sh\nif [ \"$1\" = --help ]; then echo '--settings --setting-sources --strict-mcp-config'; exit 0; fi\nexit 1\n").unwrap();
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

/// A fixture root outside every session-writable root the sandbox preflight
/// refuses: `/tmp` and `$TMPDIR` (`session_writable_roots`). That is the
/// runner-owned trusted root when `LOOM_TEST_ROOT` names one, else the
/// crate's `target/` — unless the checkout itself lives under `/tmp` or
/// `$TMPDIR`, where `target/` would be refused too and a private directory
/// under `/var/tmp` stands in.
fn trusted_fixture_root() -> PathBuf {
    if let Some(root) = std::env::var_os("LOOM_TEST_ROOT") {
        return PathBuf::from(root).join("trusted");
    }
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    let session_writable = |path: &Path| {
        let tmpdir = std::env::temp_dir();
        let tmpdir = tmpdir.canonicalize().unwrap_or(tmpdir);
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        path.starts_with("/tmp") || path.starts_with(&tmpdir)
    };
    if !session_writable(Path::new(env!("CARGO_MANIFEST_DIR"))) {
        return target;
    }
    use std::os::unix::fs::DirBuilderExt;
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let root = PathBuf::from(format!("/var/tmp/loom-tests-{uid}"));
    let _ = std::fs::DirBuilder::new().mode(0o700).create(&root);
    root.join("trusted")
}

/// Installs this build's hook scripts into a fresh directory under
/// [`trusted_fixture_root`], then points `LOOM_HOOKS_DIR` at it for the
/// duration of the guard, restoring the previous value (or clearing it) on
/// drop — including on a panic, mirroring [`ClaudeOnPathGuard`].
///
/// The sandbox preflight now runs on every spawn: check 2 refuses a host with
/// no verified hooks directory, or one whose scripts have drifted from this
/// build, and check 3 refuses a hooks directory that lies under a
/// session-writable root. Depending on the ambient `~/.claude/hooks/loom`
/// (absent on CI, and drifted from this build whenever the hooks change)
/// would make this test flaky or wrongly refused before it ever reaches the
/// tmux step it exists to pin — this guard gives it a directory that always
/// verifies.
struct LoomHooksDirGuard {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl LoomHooksDirGuard {
    fn install(root: &Path) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("tmux-abort-hooks-")
            .tempdir_in(root)
            .expect("trusted fixture root must accept a temp dir");
        crate::fs::permissions::install_loom_hooks_to(dir.path())
            .expect("installing this build's hook scripts must succeed");

        let original = std::env::var_os("LOOM_HOOKS_DIR");
        std::env::set_var("LOOM_HOOKS_DIR", dir.path());
        Self {
            _dir: dir,
            original,
        }
    }
}

impl Drop for LoomHooksDirGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("LOOM_HOOKS_DIR", value),
            None => std::env::remove_var("LOOM_HOOKS_DIR"),
        }
    }
}

/// Points the launch's `LOOM_BIN` at a stub binary under the trusted fixture
/// root for the duration of the guard, clearing the override on drop —
/// including on a panic.
///
/// `LaunchHost::from_env` otherwise resolves `LOOM_BIN` from
/// `std::env::current_exe()`, and the preflight's `accepted_loom_bin` refuses
/// a binary that is group- or world-writable (a permissive umask leaves a
/// compiled test binary at mode 775) or that lies under `/tmp` (any checkout
/// there). Either would fail the launch at that check instead of at the tmux
/// step the test exists to pin. The spawn fails before any hook runs, so the
/// stub is never executed.
struct LoomBinGuard {
    _dir: tempfile::TempDir,
}

impl LoomBinGuard {
    fn install(root: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new()
            .prefix("tmux-abort-bin-")
            .tempdir_in(root)
            .expect("trusted fixture root must accept a temp dir");
        let bin = dir.path().join("loom");
        std::fs::write(&bin, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        *launch::TEST_LOOM_BIN.lock().unwrap() = Some(bin);
        Self { _dir: dir }
    }
}

impl Drop for LoomBinGuard {
    fn drop(&mut self) {
        if let Ok(mut bin) = launch::TEST_LOOM_BIN.lock() {
            *bin = None;
        }
    }
}

/// Installs both host-readiness guards a real launch attempt needs before it
/// can reach the tmux step: a verified hooks directory and an accepted
/// `LOOM_BIN`, both outside every session-writable root.
fn host_ready_for_real_launch() -> (LoomHooksDirGuard, LoomBinGuard) {
    let root = trusted_fixture_root();
    std::fs::create_dir_all(&root).unwrap();
    (
        LoomHooksDirGuard::install(&root),
        LoomBinGuard::install(&root),
    )
}

#[test]
#[serial]
fn a_failed_spawn_aborts_and_leaves_no_pid_file() {
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
    let (_hooks, _loom_bin_mode) = host_ready_for_real_launch();
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
    let (_, pid_key) = launch::tracking_and_pid_key(&session).unwrap();

    // Stands in for a wrapper that already ran and recorded its PID before the
    // spawn failed. NOTHING but `abort_tmux_spawn` deletes this file — the
    // spawn path never writes it and never truncates it — so its absence
    // afterwards is precisely the proof that `spawn` reached its abort path.
    std::fs::create_dir_all(work.path().join("pids")).unwrap();
    let pid_file = work.path().join("pids").join(format!("{pid_key}.pid"));
    std::fs::write(&pid_file, "424242\n").unwrap();
    let (socket, err) = failed_spawn(work.path(), &stage, session);
    assert_abort_cleanup(&err, &socket, &pid_file, work.path(), &pid_key);
}

fn failed_spawn(work: &Path, stage: &Stage, session: Session) -> (String, anyhow::Error) {
    let worktree = Worktree::new(
        stage.id.clone(),
        work.to_path_buf(),
        format!("loom/{}", stage.id),
    );
    let socket = socket_name(&session);
    let _server = TmuxServerGuard::new(&socket);
    let signal = work.join("signal.md");
    let err = TmuxBackend::new(work.to_path_buf())
        .spawn_session(SessionType::Stage, stage, &worktree, session, &signal)
        .expect_err("tmux spawn or PID discovery must fail");
    (socket, err)
}

fn assert_abort_cleanup(
    err: &anyhow::Error,
    socket: &str,
    pid_file: &Path,
    work: &Path,
    pid_key: &str,
) {
    // POSITIVE CONTROL. Both assertions below would also hold if the spawn had
    // died inside `prepare_session_launch` — before any abort was owed. These
    // are the two post-preparation shapes: every `spawn_in_tmux` error names
    // the socket, and `await_session_pid`'s names the pid_key it gave up on.
    // Neither string can come out of `prepare_session_launch`, whose failures
    // report a missing claude binary or an unwritable wrapper path.
    let message = err.to_string();
    assert!(
        message.contains(socket) || message.contains("No PID discovered"),
        "the spawn must have failed at or after the tmux step, not before it: {err}"
    );

    assert!(
        !pid_file.exists(),
        "the failed spawn must delete the PID file under its own pid_key \
         (spawn error was: {err})"
    );
    assert!(
        !work
            .join("wrappers")
            .join(format!("{pid_key}-wrapper.sh"))
            .exists(),
        "the wrapper script this spawn created must be removed on the failure path too"
    );
}
