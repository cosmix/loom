//! Unit tests for `backend.rs`, split out to keep the module under the
//! 400-line ceiling (CLAUDE.md Rule 17).
//!
//! `dispatch_spawn` is private, but this module is a child of `backend`, so
//! the dispatch decision can be driven directly with closures instead of being
//! approximated through the four public `spawn_*` wrappers (which would need a
//! real `Stage`, `Worktree`, signal file, and terminal).

use super::*;
use crate::models::session::TerminalConfig;
use crate::orchestrator::terminal::emulator::TerminalEmulator;
use serial_test::serial;
use tempfile::TempDir;

/// A `SessionBackend` with an injected tmux-availability probe and NO native
/// lane resolved yet — enough for the pure lane-resolution tests.
///
/// Anything that can reach the native lane must seed it first
/// ([`seed_native_available`] / [`seed_native_unavailable`]), otherwise the
/// test's outcome depends on whether the machine running it happens to have a
/// terminal emulator.
fn test_backend(
    work_dir: PathBuf,
    configured_kind: SessionBackendKind,
    tmux_available: fn() -> bool,
) -> SessionBackend {
    SessionBackend {
        tmux: TmuxBackend::new(work_dir.clone()),
        work_dir,
        configured_kind,
        native: None,
        lazy_native: OnceLock::new(),
        tmux_available,
    }
}

/// Pre-populate the memoized native lane with a usable backend, making "the
/// native lane is available" a property of the test rather than of the host.
fn seed_native_available(backend: &SessionBackend, work_dir: &Path) {
    let native = NativeBackend::with_terminal(TerminalEmulator::XTerm, work_dir.to_path_buf());
    assert!(
        backend.lazy_native.set(Ok(native)).is_ok(),
        "the native lane must not have been resolved before seeding"
    );
}

/// The headless counterpart: terminal detection failed, the way
/// `NativeBackend::new` reports it on a box with no terminal emulator.
fn seed_native_unavailable(backend: &SessionBackend) {
    assert!(
        backend
            .lazy_native
            .set(Err("No terminal emulator found".to_string()))
            .is_ok(),
        "the native lane must not have been resolved before seeding"
    );
}

fn failing_tmux(_tmux: &TmuxBackend, _session: Session) -> Result<Session> {
    anyhow::bail!("tmux new-session failed for socket 'loom-x': boom")
}

#[test]
fn resolve_lane_never_depends_on_tmux_availability() {
    // The configured backend is the only input: unavailable tmux is a
    // spawn-time hard error now, never a reason to resolve to a different
    // lane ahead of time.
    let temp = TempDir::new().unwrap();
    let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || {
        false
    });
    assert_eq!(backend.resolve_lane(), SessionBackendKind::Tmux);
}

#[test]
fn tmux_configured_resolves_tmux_lane() {
    let temp = TempDir::new().unwrap();
    let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || true);
    assert_eq!(backend.resolve_lane(), SessionBackendKind::Tmux);
}

#[test]
fn native_configured_always_resolves_native_lane_even_if_tmux_available() {
    let temp = TempDir::new().unwrap();
    let backend = test_backend(
        temp.path().to_path_buf(),
        SessionBackendKind::Native,
        || true,
    );
    assert_eq!(backend.resolve_lane(), SessionBackendKind::Native);
}

#[test]
fn tmux_spawn_failure_returns_err_and_never_retries_native() {
    let temp = TempDir::new().unwrap();
    let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || true);
    let session = Session::new();
    let session_id = session.id.clone();

    let err = backend
        .dispatch_spawn(
            session,
            |_native, _s| panic!("a tmux spawn failure must never fall back to native"),
            failing_tmux,
        )
        .expect_err("a failing tmux spawn must return Err");

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("tmux new-session failed"),
        "the original tmux error is the caller's only useful diagnostic, got: {rendered}"
    );
    assert!(
        rendered.contains(&session_id),
        "the error must name the session that failed to spawn, got: {rendered}"
    );
    assert!(
        std::fs::read_dir(temp.path()).unwrap().next().is_none(),
        "a tmux spawn failure must write nothing to the work dir"
    );
}

#[test]
fn tmux_configured_unavailable_returns_actionable_err_and_never_spawns() {
    let temp = TempDir::new().unwrap();
    let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || {
        false
    });

    let err = backend
        .dispatch_spawn(
            Session::new(),
            |_native, _s| panic!("an unavailable tmux must not fall back to native"),
            |_tmux, _s| panic!("tmux must not be spawned when unavailable"),
        )
        .expect_err("configured tmux with no tmux on PATH must fail");

    let rendered = err.to_string();
    assert!(
        rendered.contains("tmux is not on PATH"),
        "must name the problem, got: {rendered}"
    );
    assert!(
        rendered.contains("[terminal] backend = \"native\""),
        "must name the fix, got: {rendered}"
    );
    assert!(
        std::fs::read_dir(temp.path()).unwrap().next().is_none(),
        "an unavailable tmux must write nothing to the work dir"
    );
}

#[test]
fn tmux_lane_stamps_the_tmux_backend() {
    let temp = TempDir::new().unwrap();
    let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || true);

    let spawned = backend
        .dispatch_spawn(
            Session::new(),
            |_native, _s| panic!("the native lane must not run when tmux succeeds"),
            |_tmux, s| {
                assert_eq!(
                    s.backend,
                    SessionBackendKind::Tmux,
                    "the lane ACTUALLY used must be stamped before delegating"
                );
                Ok(s)
            },
        )
        .unwrap();

    assert_eq!(spawned.backend, SessionBackendKind::Tmux);
}

#[test]
fn native_lane_stamps_the_native_backend() {
    let temp = TempDir::new().unwrap();
    let backend = test_backend(
        temp.path().to_path_buf(),
        SessionBackendKind::Native,
        || true,
    );
    seed_native_available(&backend, temp.path());

    let spawned = backend
        .dispatch_spawn(
            Session::new(),
            |_native, s| {
                assert_eq!(s.backend, SessionBackendKind::Native);
                Ok(s)
            },
            |_tmux, _s| panic!("a configured-native backend must never touch the tmux lane"),
        )
        .unwrap();

    assert_eq!(spawned.backend, SessionBackendKind::Native);
}

#[test]
#[serial]
fn the_lazy_native_lane_is_built_at_most_once() {
    // `#[serial]`: reaching an unseeded lane runs `detect_terminal`, which
    // reads env vars `native::detection`'s tests mutate.
    //
    // Pointer identity is the assertion because it is the only thing that
    // distinguishes a memoized lane from a freshly constructed one. Swapping
    // the `OnceLock` back for per-call construction makes both arms fail —
    // which is the point: `is_session_alive` reaches this once per native
    // session on every 5-second monitor tick.
    let temp = TempDir::new().unwrap();
    let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || true);

    match (backend.native_lane(), backend.native_lane()) {
        (Ok(first), Ok(second)) => assert!(
            std::ptr::eq(first, second),
            "the native lane must be constructed once and reused"
        ),
        (Err(first), Err(second)) => assert!(
            std::ptr::eq(first.as_ptr(), second.as_ptr()),
            "the construction FAILURE must be memoized too, not re-probed per call"
        ),
        _ => panic!("the memoized lane must not change verdict between calls"),
    }
}

#[test]
fn a_native_session_degrades_to_pid_only_when_no_terminal_exists() {
    // Headless: `native_lane()` is `Err`, so kill/liveness must fall through
    // to the shared PID layers rather than propagating the construction
    // error. Erroring here would make the monitor treat every poll as a
    // failure and `kill_session` unable to reap anything on a tmux host.
    let temp = TempDir::new().unwrap();
    let backend = test_backend(
        temp.path().to_path_buf(),
        SessionBackendKind::Native,
        || false,
    );
    seed_native_unavailable(&backend);

    let mut session = Session::new();
    session.assign_to_stage("headless-stage".to_string());
    session.backend = SessionBackendKind::Native;
    session.pid = Some(999_999_999);
    super::super::native::write_test_pid_identity(temp.path(), &session, session.pid.unwrap())
        .unwrap();

    assert!(
        !backend.is_session_alive(&session).unwrap(),
        "no PID evidence and a dead stored PID must read as dead, not as an error"
    );
    backend
        .kill_session(&session)
        .expect("teardown with nothing to signal is a success");
}

#[test]
fn from_config_tmux_leaves_the_native_lane_unbuilt() {
    // The whole point of the lazy lane: choosing tmux must not require a
    // terminal emulator, or `from_config` would fail on exactly the headless
    // hosts tmux exists to serve.
    let temp = TempDir::new().unwrap();
    crate::fs::work_dir::write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();

    let backend = SessionBackend::from_config(temp.path().to_path_buf()).unwrap();

    assert!(
        backend.native.is_none(),
        "configured tmux must not eagerly construct the native lane"
    );
    assert_eq!(backend.configured_kind, SessionBackendKind::Tmux);
    // The configured lane is authoritative regardless of real tmux
    // availability on the machine running this test.
    assert_eq!(backend.resolve_lane(), SessionBackendKind::Tmux);
}

#[test]
#[serial]
fn from_config_native_builds_the_lane_eagerly_or_fails_with_detection() {
    // `#[serial]`: `detect_terminal` reads LOOM_TERMINAL / TERMINAL /
    // TERM_PROGRAM, which `native::detection`'s tests mutate process-globally.
    let temp = TempDir::new().unwrap();
    crate::fs::work_dir::write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Native,
        },
    )
    .unwrap();

    // Stated as an equivalence rather than "native.is_some()" so it holds on a
    // headless CI box too: detection succeeding and the lane being built
    // eagerly are the same event. The state that must never exist is a
    // configured-native backend with neither a lane nor an error, which would
    // silently defer the failure to the first spawn.
    let detected = crate::orchestrator::terminal::native::detect_terminal().is_ok();
    match SessionBackend::from_config(temp.path().to_path_buf()) {
        Ok(backend) => {
            assert!(detected, "from_config succeeded though detection failed");
            assert!(
                backend.native.is_some(),
                "configured native must build the lane eagerly"
            );
            assert_eq!(backend.resolve_lane(), SessionBackendKind::Native);
        }
        Err(_) => assert!(
            !detected,
            "from_config must only fail when terminal detection fails"
        ),
    }
}
