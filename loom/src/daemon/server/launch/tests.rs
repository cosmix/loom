//! `await_ready` against `/bin/sh` children that play the daemon child's part
//! on the readiness pipe, plus the spawn guard and the daemon's command line.

use super::*;
use nix::sys::signal::Signal;
use std::ffi::OsStr;
use std::os::unix::process::ExitStatusExt;
use tempfile::TempDir;

/// Kills and reaps the child when the test ends, pass or fail.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `/bin/sh -c 'ulimit -c 0; <script>' <arg0>`, stdin null, stdout and
/// stderr on one pipe whose read end is returned.
fn spawn_script(script: &str, arg0: &Path) -> (Reaped, PipeReader) {
    let (reader, writer) = std::io::pipe().expect("create pipe");
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!("ulimit -c 0; {script}"))
        .arg(arg0)
        .stdin(Stdio::null())
        .stdout(writer.try_clone().expect("clone pipe writer"))
        .stderr(writer);
    let child = command.spawn().expect("spawn sh");
    drop(command);
    (Reaped(child), reader)
}

fn timing(deadline_ms: u64, grace_ms: u64) -> ReadyTiming {
    ReadyTiming {
        deadline: Duration::from_millis(deadline_ms),
        grace: Duration::from_millis(grace_ms),
    }
}

/// Runs `script` with `$0` set to a log path in a fresh temp dir and returns
/// `await_ready`'s error text.
fn launch_failure(script: &str, timing: ReadyTiming) -> String {
    let dir = TempDir::new().expect("temp dir");
    let log = dir.path().join("orchestrator.log");
    let (mut child, reader) = spawn_script(script, &log);
    match await_ready(&mut child.0, reader, &log, timing) {
        Ok(()) => panic!("`{script}` must not be accepted as a ready daemon"),
        Err(error) => format!("{error:#}"),
    }
}

#[test]
fn a_ready_child_that_stays_alive_is_accepted() {
    let dir = TempDir::new().expect("temp dir");
    let log = dir.path().join("orchestrator.log");
    let (mut child, reader) = spawn_script("printf '\\001'; exec sleep 30", &log);

    let result = await_ready(&mut child.0, reader, &log, timing(2000, 100));

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn an_early_exit_reports_status_and_text() {
    let error = launch_failure("printf lockfail >&2; exit 3", timing(2000, 100));

    assert!(error.contains("exit status 3"), "{error}");
    assert!(error.contains("lockfail"), "{error}");
}

#[test]
fn diagnostic_text_keeps_only_the_last_64_kib() {
    let script = "yes filler | head -c 200000 >&2; printf TAILMARK >&2; exit 3";

    let error = launch_failure(script, timing(2000, 100));

    assert!(error.contains("TAILMARK"), "the tail of the text is kept");
    assert!(error.contains("exit status 3"), "the status is still named");
    assert!(
        error.len() < MAX_DIAGNOSTIC_BYTES + 512,
        "the text is capped at {MAX_DIAGNOSTIC_BYTES} bytes, got {}",
        error.len()
    );
}

#[test]
fn ready_then_abort_names_the_signal_and_the_log_tail() {
    let script = "printf '\\002'; echo boom >\"$0\"; printf '\\001'; kill -s ABRT $$";

    let error = launch_failure(script, timing(2000, 2000));

    assert!(error.contains("SIGABRT"), "{error}");
    assert!(error.contains("boom"), "{error}");
}

#[test]
fn a_silent_child_times_out_and_is_reaped() {
    let dir = TempDir::new().expect("temp dir");
    let log = dir.path().join("orchestrator.log");
    let (mut child, reader) = spawn_script("exec sleep 30", &log);

    let result = await_ready(&mut child.0, reader, &log, timing(200, 100));

    let error = format!("{:#}", result.expect_err("a silent child is not ready"));
    assert!(error.contains("did not become ready"), "{error}");
    assert!(
        matches!(child.0.try_wait(), Ok(Some(_))),
        "the timed-out child is terminated and reaped"
    );
}

#[test]
fn a_timed_out_child_reports_what_it_wrote_on_termination() {
    let script = "trap 'printf LASTWORDS; exit 1' TERM; while :; do sleep 0.1; done";

    let error = launch_failure(script, timing(300, 100));

    assert!(error.contains("did not become ready"), "{error}");
    assert!(
        error.contains("LASTWORDS"),
        "the pipe is drained after the reap: {error}"
    );
}

#[test]
fn a_pipe_closed_while_the_child_lives_terminates_and_reaps_it() {
    let dir = TempDir::new().expect("temp dir");
    let log = dir.path().join("orchestrator.log");
    let (mut child, reader) = spawn_script("exec >&- 2>&-; exec sleep 30", &log);

    let result = await_ready(&mut child.0, reader, &log, timing(5000, 100));

    let error = format!("{:#}", result.expect_err("a closed pipe is not ready"));
    assert!(
        error.contains("closed its output before it was ready"),
        "{error}"
    );
    let status = child
        .0
        .try_wait()
        .expect("check the child")
        .expect("the child is reaped");
    assert_eq!(status.signal(), Some(Signal::SIGTERM as i32), "{status:?}");
}

#[test]
fn the_log_tail_is_made_safe_for_the_terminal() {
    let script = "printf '\\002'; printf 'evil\\033]52;c;AAAA\\007tail\\n' >\"$0\"; exit 3";

    let error = launch_failure(script, timing(2000, 100));

    assert!(error.contains("evil"), "{error}");
    assert!(
        !error.contains('\u{1b}'),
        "no escape reaches the terminal: {error:?}"
    );
    assert!(
        !error.contains('\u{7}'),
        "no bell reaches the terminal: {error:?}"
    );
}

#[test]
fn a_suppressed_spawn_is_counted_and_starts_nothing() {
    let dir = TempDir::new().expect("temp dir");
    let before = SUPPRESSED_SPAWNS.load(Ordering::SeqCst);

    spawn_daemon(dir.path(), &DaemonConfig::default()).expect("a suppressed spawn succeeds");

    assert!(SUPPRESSED_SPAWNS.load(Ordering::SeqCst) > before);
    let entries = std::fs::read_dir(dir.path())
        .expect("read temp dir")
        .count();
    assert_eq!(entries, 0, "a suppressed spawn writes nothing");
}

#[test]
fn the_daemon_command_carries_flag_root_config_and_a_clean_environment() {
    let root = Path::new("/repo/.loom/work");
    let env = DaemonEnvironment::capture_from([
        ("HOME", "/h"),
        ("RUST_LOG", "debug"),
        ("SCCACHE_DIR", "/s"),
        ("LOOM_ADMIN_TOKEN", "x"),
    ]);
    let config = DaemonConfig {
        manual_mode: true,
        max_parallel: Some(2),
        auto_merge: false,
        ..DaemonConfig::default()
    };

    let command = daemon_command(
        Path::new("/usr/bin/loom"),
        root,
        &config,
        &env,
        Some("kitty"),
    );

    let args: Vec<&OsStr> = command.get_args().collect();
    let expected = [
        "run",
        "--daemon-child",
        "/repo/.loom/work",
        "--manual",
        "-p",
        "2",
        "--no-merge",
    ]
    .map(OsStr::new);
    assert_eq!(args, expected);
    let envs: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
    for name in ["HOME", "RUST_LOG", "SCCACHE_DIR"] {
        assert!(envs.iter().any(|(key, _)| *key == name), "{name} is passed");
    }
    assert!(envs.contains(&(OsStr::new("LOOM_TERMINAL"), Some(OsStr::new("kitty")))));
    assert!(!envs.iter().any(|(key, _)| *key == "LOOM_ADMIN_TOKEN"));
    let rendered = format!("{command:?}");
    assert!(rendered.starts_with("env -i"), "{rendered}");
}
