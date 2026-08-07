//! Unit tests for `tmux/mod.rs`, split out to keep the module under the
//! 400-line ceiling (CLAUDE.md Rule 17).

use super::*;
use std::path::PathBuf;

#[test]
fn socket_name_has_loom_prefix_and_fits_sun_path() {
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
fn tmux_liveness_ignores_running_server_when_pid_is_dead() {
    // No tmux server exists for this socket at all, and the PID is
    // bogus. The point is that is_session_alive never asks tmux —
    // it must return false purely from the (absent/dead) PID evidence.
    let temp = tempfile::TempDir::new().unwrap();
    let backend = TmuxBackend::new(temp.path().to_path_buf());

    let mut session = Session::new();
    session.assign_to_stage("dead-stage".to_string());
    session.pid = Some(999_999_999);

    assert!(!backend.is_session_alive(&session).unwrap());
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
    assert!(evaluate_new_session(
        "loom-abc",
        false,
        "couldn't create directory /x/tmux-501 (Permission denied)"
    )
    .is_err());
}
