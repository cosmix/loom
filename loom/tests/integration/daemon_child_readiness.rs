//! The daemon child's side of the readiness pipe, through the real binary:
//! `loom run --daemon-child` writes the log-active byte once its output goes
//! to `orchestrator.log`, and a failure after that point lands in the log
//! rather than on the pipe.
//!
//! The failure is a socket path past the `sun_path` limit: `run_server`
//! refuses it after the log-active byte and before any bind, so the test
//! needs no AF_UNIX permission and leaves no daemon behind.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use loom::daemon::{SOCKET_FILE, SUN_PATH_MAX};
use tempfile::TempDir;

use super::helpers::loom_cmd;

/// `daemon::server::launch::LOG_ACTIVE_BYTE`, which is crate-private.
const LOG_ACTIVE_BYTE: u8 = 0x02;
/// The child refuses within milliseconds; this only bounds a regression.
const CHILD_DEADLINE: Duration = Duration::from_secs(30);

/// Kills and reaps the child when the test ends, pass or fail. A child
/// already reaped is left alone.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `loom run --daemon-child <work_root>` from `cwd`, with a scratch `HOME`
/// and `LOOM_HOME` and none of this process's `LOOM_*` variables (a test run
/// inside a loom session carries `LOOM_STAGE_ID` and `LOOM_SESSION_ID`).
/// stdout is piped; stderr goes to `cwd/stderr.txt`.
fn daemon_child_command(cwd: &Path, work_root: &Path, home: &Path) -> Command {
    let mut command = loom_cmd();
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(name);
        }
    }
    let stderr = File::create(cwd.join("stderr.txt")).expect("create the stderr capture");
    command
        .args(["run", "--daemon-child"])
        .arg(work_root)
        .current_dir(cwd)
        .env("HOME", home)
        .env("LOOM_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(stderr);
    command
}

/// Everything written to `stdout` up to EOF, or `None` when EOF does not
/// come within [`CHILD_DEADLINE`].
fn read_to_eof(mut stdout: ChildStdout) -> Option<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = sender.send(stdout.read_to_end(&mut bytes).map(|_| bytes));
    });
    let read = receiver.recv_timeout(CHILD_DEADLINE).ok()?;
    Some(read.expect("read the readiness pipe"))
}

/// The child's exit status, or `None` when it is still running after
/// [`CHILD_DEADLINE`].
fn exit_within_deadline(child: &mut Child) -> Option<ExitStatus> {
    let started = Instant::now();
    while started.elapsed() < CHILD_DEADLINE {
        if let Some(status) = child.try_wait().expect("check the daemon child") {
            return Some(status);
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
fn the_daemon_child_reports_log_active_then_fails_into_the_log() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().canonicalize().expect("canonical temp dir");
    let work_root = root.join("w".repeat(SUN_PATH_MAX));
    let home = root.join("home");
    fs::create_dir_all(&work_root).expect("create the state root");
    fs::create_dir_all(&home).expect("create the scratch home");
    assert!(work_root.join(SOCKET_FILE).as_os_str().len() >= SUN_PATH_MAX);

    let mut command = daemon_child_command(&root, &work_root, &home);
    let mut child = Reaped(command.spawn().expect("spawn the daemon child"));
    let stdout = child.0.stdout.take().expect("piped stdout");

    let written = read_to_eof(stdout).expect("the readiness pipe reaches EOF");
    let status = exit_within_deadline(&mut child.0).expect("the daemon child exits");

    let stderr = fs::read_to_string(root.join("stderr.txt")).unwrap_or_default();
    assert_eq!(written, [LOG_ACTIVE_BYTE], "stderr: {stderr}");
    assert!(
        status.code().is_some_and(|code| code != 0),
        "a non-zero exit, not a signal: {status:?}; stderr: {stderr}"
    );
    let log = fs::read_to_string(work_root.join("orchestrator.log")).expect("read the daemon log");
    assert!(
        log.contains(&format!("exceeds the {SUN_PATH_MAX}-byte sun_path limit")),
        "the socket-path error is in the log: {log}"
    );
}
