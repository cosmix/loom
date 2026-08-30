//! Unit tests for `tmux/mod.rs`, split out to keep the module under the
//! 400-line ceiling (CLAUDE.md Rule 17).

use super::socket::tests::TmuxTmpDirGuard;
use super::*;
use serial_test::serial;
use std::path::PathBuf;

#[test]
#[serial]
fn socket_name_has_loom_prefix_and_fits_sun_path() {
    // `socket_path_for` reads $TMUX_TMPDIR, which socket.rs's tests mutate
    // process-globally. Left unpinned, this test measures the bound against
    // whatever directory a concurrently running test happened to install —
    // a long `TempDir` path would fail it spuriously, and a short one would
    // let a genuinely oversized default through. Pin tmux's OWN default so
    // the 104-byte budget is measured where real sockets actually land.
    let _guard = TmuxTmpDirGuard::set(Path::new("/tmp"));

    let session = Session::new();
    let name = socket_name(&session);
    assert!(name.starts_with("loom-"));
    // AF_UNIX sun_path is 104 bytes on macOS/BSD (108 on Linux); use the
    // tighter bound so the socket fits everywhere loom runs.
    let path = socket_path_for(&name);
    assert!(
        path.as_os_str().len() < 104,
        "socket path '{}' ({} bytes) must stay under the AF_UNIX sun_path limit",
        path.display(),
        path.as_os_str().len()
    );
}

/// Round-trip an escaped argv entry through a real POSIX shell
/// (`sh -c 'printf %s <escaped>'`) and return what it printed. This
/// proves the escaping is both safe (the shell doesn't choke on it) and
/// correct (it decodes back to the original string) without hardcoding
/// `shell_escape`'s internal algorithm into the test.
fn shell_round_trip(escaped: &str) -> String {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("printf '%s' {escaped}"))
        .output()
        .expect("sh should be available to round-trip the escaped argument");
    assert!(
        output.status.success(),
        "sh -c failed on escaped argv entry '{escaped}': {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("printf output should be valid UTF-8")
}

#[test]
fn new_session_argv_escapes_space_in_command() {
    let cmd = PathBuf::from("/tmp/loom wrapper.sh");
    let argv = new_session_argv("sock", "sess", Path::new("/tmp"), &cmd);
    let escaped = argv.last().unwrap();
    assert_eq!(shell_round_trip(escaped), cmd.to_string_lossy());
}

#[test]
fn new_session_argv_escapes_single_quote_in_command() {
    let cmd = PathBuf::from("/tmp/loom's wrapper.sh");
    let argv = new_session_argv("sock", "sess", Path::new("/tmp"), &cmd);
    let escaped = argv.last().unwrap();
    // A raw, unescaped single quote would terminate a single-quoted shell
    // string early and split the command; the round trip through a real
    // shell proves it decodes back to the exact original path instead.
    assert_eq!(shell_round_trip(escaped), cmd.to_string_lossy());
}

#[test]
fn new_session_argv_escapes_dollar_and_semicolon() {
    let cmd = PathBuf::from("/tmp/loom$wrapper;rm.sh");
    let argv = new_session_argv("sock", "sess", Path::new("/tmp"), &cmd);
    let escaped = argv.last().unwrap();
    // Unescaped, `$wrapper` would expand (to empty) and `;` would end the
    // command early. The round trip proves both are inert.
    assert_eq!(shell_round_trip(escaped), cmd.to_string_lossy());
}

#[test]
fn new_session_argv_keeps_command_substitution_and_newline_inert() {
    let cmd = PathBuf::from("/tmp/loom $(printf injected);wrapper\nnext.sh");
    let argv = new_session_argv("sock", "sess", Path::new("/tmp"), &cmd);
    assert_eq!(
        shell_round_trip(argv.last().unwrap()),
        cmd.to_string_lossy()
    );
}

#[test]
fn tmux_control_runner_returns_structured_timeout() {
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 60"]);
    let error = run_tmux_command(
        &mut command,
        std::time::Duration::from_millis(100),
        "tmux deterministic timeout",
        &[],
    )
    .expect_err("fake tmux command must exceed its deadline");

    let timeout = error
        .downcast_ref::<crate::process::ProcessTimeoutError>()
        .expect("tmux timeout must stay machine-identifiable");
    assert_eq!(timeout.operation(), "tmux deterministic timeout");
}

/// Asserts on the CONFIGURED `Command`, never on a subprocess's output: this
/// test does not run tmux and does not touch `std::env`. Setting a real
/// global `TERM` to prove the pin overrides an inherited value would leak
/// that value into every OTHER test running concurrently in this binary — a
/// prior version of this test did exactly that and produced an intermittent,
/// unrelated failure elsewhere in the suite.
///
/// The override winning is structural, not something this test needs to
/// force: `apply_stage_environment` does `env_clear()` and repopulates from
/// the host's allowlist, and `tmux_control_command` applies
/// `CONTROL_TERM_OVERRIDE` after that — `Command`'s env is a key/value map,
/// so the later `.env("TERM", "dumb")` call always wins regardless of
/// whatever TERM (if any) the host forwarded. Note honestly: on a host whose
/// OWN ambient TERM happens to already be `dumb`, this assertion would pass
/// even if the override were deleted — an acceptable trade for not mutating
/// process-global state.
#[test]
fn control_command_pins_term_dumb() {
    let command = tmux_control_command(&["has-session", "-t", "x"]);
    let term = command
        .get_envs()
        .find(|(key, _)| *key == std::ffi::OsStr::new("TERM"))
        .map(|(_, value)| value);
    assert_eq!(
        term,
        Some(Some(std::ffi::OsStr::new("dumb"))),
        "tmux control commands must pin TERM=dumb so a pure control query \
         never depends on a resolvable terminal"
    );
}

#[test]
fn is_session_alive_is_false_for_a_dead_pid_with_no_server() {
    // Scope note, so this test is not mistaken for the containment proof it
    // is NOT: no tmux server exists for this socket, so it cannot show that
    // `is_session_alive` ignores a RUNNING server whose pane died — it would
    // pass unchanged even if the implementation did call `has-session`. What
    // it does pin is the base case: with no PID identity evidence, the answer
    // is false rather than an error or a default "alive". The real containment
    // test starts an actual server and lives in
    // `tests/e2e/tmux_backend.rs`.
    let temp = tempfile::TempDir::new().unwrap();
    let backend = TmuxBackend::new(temp.path().to_path_buf());

    let mut session = Session::new();
    session.assign_to_stage("dead-stage".to_string());
    session.pid = Some(999_999_999);

    assert!(!backend.is_session_alive(&session).unwrap());
}

#[test]
#[serial]
fn kill_session_retains_an_unverifiable_stale_socket() {
    let socket_dir = tempfile::TempDir::new().unwrap();
    let _tmux_tmpdir = TmuxTmpDirGuard::set(socket_dir.path());
    let work_dir = tempfile::TempDir::new().unwrap();
    let backend = TmuxBackend::new(work_dir.path().to_path_buf());

    let mut session = Session::new();
    session.assign_to_stage("dead-socket".to_string());
    native::write_test_pid_identity(work_dir.path(), &session, 999_999_999).unwrap();

    let socket = socket_path_for(&socket_name(&session));
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    std::fs::write(&socket, "").unwrap();

    let error = backend.kill_session(&session).unwrap_err();
    assert!(error.to_string().contains("tmux kill-server failed"));
    assert!(
        socket.exists(),
        "failed server teardown must retain the only possible control handle"
    );
}

#[test]
fn new_session_succeeds_only_on_clean_exit_and_empty_stderr() {
    assert!(evaluate_new_session("sock", true, "").is_ok());
    assert!(evaluate_new_session("sock", true, "   \n").is_ok());
}

#[test]
fn new_session_exit_zero_with_stderr_is_a_failure() {
    // THE REGRESSION THIS PINS: tmux 3.7b prints
    // `error creating <path> (Operation not permitted)` and STILL EXITS 0
    // when it cannot create its socket. An exit-code-only check reports
    // that total failure as success. If someone "simplifies" the check
    // back to `!status.success()`, this test is what fails.
    let err = evaluate_new_session(
        "loom-abc",
        true,
        "error creating /private/tmp/tmux-501/loom-abc (Operation not permitted)",
    )
    .expect_err("exit 0 with stderr must be treated as a failure");
    // The captured stderr is carried verbatim so the operator sees the
    // real reason rather than a generic "spawn failed".
    assert!(err.to_string().contains("Operation not permitted"));
    assert!(err.to_string().contains("loom-abc"));
}

#[test]
fn new_session_nonzero_exit_is_a_failure() {
    // The other real-world shape: an unwritable socket parent makes tmux
    // exit 1 with `couldn't create directory … (Permission denied)`.
    let err = evaluate_new_session(
        "loom-abc",
        false,
        "couldn't create directory /x/tmux-501 (Permission denied)",
    )
    .expect_err("a non-zero exit must be treated as a failure");
    // Same contract as the exit-0-with-stderr case: the captured stderr is
    // carried verbatim into the error. `dispatch_spawn` prints this text as
    // the operator's ONLY explanation of why tmux was abandoned, so dropping
    // it would leave "tmux backend spawn failed" with no reason attached.
    assert!(
        err.to_string().contains("Permission denied"),
        "error must carry tmux's stderr, got: {err}"
    );
    assert!(err.to_string().contains("loom-abc"));
}

#[test]
fn stage_servers_force_mouse_capture_off() {
    // tmux reads the operator's `~/.tmux.conf` at `start-server`, so whatever
    // it sets is live inside every agent pane loom creates. `set -g mouse on`
    // is common, and with capture on a drag is eaten by tmux's copy-mode
    // instead of reaching the terminal emulator — the operator cannot select
    // an agent's output at all.
    //
    // Asserted by VALUE, not presence. The failure mode worth pinning is not
    // a missing entry but an inverted one: `("mouse", "on")` would force
    // capture on for operators who had deliberately turned it off, taking
    // their selection away rather than giving it back.
    let mouse = PRESENTATION_OPTIONS
        .iter()
        .find(|(option, _)| *option == "mouse")
        .expect("stage servers must pin the mouse option, not inherit it");
    assert_eq!(
        mouse.1, "off",
        "mouse capture must be forced OFF on servers loom creates"
    );
}

#[test]
fn stage_servers_delete_the_kmous_capability() {
    // `mouse off` is not enough on its own: claude enables all-motion mouse
    // tracking in its pane, tmux mirrors that mode out to any attached
    // client's terminal whenever that terminal has the `kmous` capability,
    // and with `mouse off` incoming client mouse input is forwarded straight
    // into the pane app. A drag then becomes app mouse events inside claude,
    // claude copies its "selection" with `tmux load-buffer -w -`, and tmux
    // 3.6a crashes serving that with a client attached — the
    // `server exited unexpectedly` stage deaths. Deleting `kmous` for every
    // client TERM stops mouse mode from ever reaching the terminal, so drags
    // stay native emulator selection and no event reaches the agent.
    //
    // Asserted by VALUE. The entry must delete the capability (`@`) for every
    // TERM (`*`), and must use an indexed slot so re-application is
    // idempotent and the operator's own override entries survive.
    let along = PRESENTATION_OPTIONS
        .iter()
        .find(|(option, _)| *option == "terminal-overrides[99]")
        .expect("stage servers must pin an indexed terminal-overrides entry");
    assert_eq!(
        along.1, "*:kmous@",
        "the override must delete kmous for every client TERM"
    );
}
