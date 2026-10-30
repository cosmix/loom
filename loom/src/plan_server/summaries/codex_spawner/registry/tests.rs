//! The forced stop: it reaches a group whose leader is gone, and a server stop waits for it.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use super::*;
use crate::plan_server::pressure::config_guard;

const SESSION: &str = "c0de-1-0";

/// `script` in a process group of its own, once it has printed its ready line.
fn group(script: &str) -> (Child, Pid, BufReader<ChildStdout>) {
    let mut child = Command::new("sh")
        .args(["-c", script])
        .stdout(Stdio::piped())
        .process_group(0)
        .spawn()
        .expect("spawn sh");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut ready = String::new();
    stdout.read_line(&mut ready).expect("ready line");
    assert_eq!(ready, "ready\n");
    let pid = Pid::from_raw(child.id() as i32);
    (child, pid, stdout)
}

/// A registry that holds a job for `pid`.
fn registered(pid: Pid) -> Jobs {
    let root = tempfile::TempDir::new().expect("temp dir");
    let job = Job {
        pid,
        before: config_guard::snapshot(root.path()).expect("config snapshot"),
        codex_before: BTreeMap::new(),
        stopping: false,
    };
    let jobs = Jobs::default();
    assert!(register(&jobs, SESSION, job));
    jobs
}

/// Fires when every process holding `stdout` has died.
fn closed(mut stdout: BufReader<ChildStdout>) -> Receiver<()> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = stdout.read_to_end(&mut Vec::new());
        let _ = tx.send(());
    });
    rx
}

#[test]
fn the_forced_stop_reaches_a_descendant_after_the_leader_left_the_registry() {
    // The leader dies of SIGTERM; its background child ignores it and holds the pipe.
    let script = "(trap '' TERM; echo ready; exec sleep 30) & wait";
    let (mut leader, pid, stdout) = group(script);
    let jobs = registered(pid);
    terminate(&jobs, SESSION);
    let status = leader.wait().expect("reap the leader");
    assert_eq!(status.signal(), Some(Signal::SIGTERM as i32));
    lock(&jobs).jobs.remove(SESSION);
    let gone = closed(stdout);
    assert!(
        gone.recv_timeout(Duration::from_secs(1)).is_err(),
        "the descendant outlives SIGTERM"
    );
    gone.recv_timeout(FORCE_AFTER + Duration::from_secs(5))
        .expect("the forced stop ended the descendant");
    assert!(lock(&jobs).stopped_groups.is_empty());
}

#[test]
fn a_server_stop_forces_a_group_that_ignores_sigterm_after_the_grace() {
    let (mut child, pid, _stdout) = group("trap '' TERM; echo ready; exec sleep 30");
    let jobs = registered(pid);
    terminate_all(&jobs);
    let started = Instant::now();
    force_stopped(&jobs, Duration::from_millis(200), &AtomicBool::new(false));
    assert!(started.elapsed() >= Duration::from_millis(200), "it waited");
    let status = child.wait().expect("reap");
    assert_eq!(status.signal(), Some(Signal::SIGKILL as i32));
}

#[test]
fn an_abort_ends_the_wait_and_still_forces() {
    let (mut child, pid, _stdout) = group("trap '' TERM; echo ready; exec sleep 30");
    let jobs = registered(pid);
    terminate_all(&jobs);
    let started = Instant::now();
    force_stopped(&jobs, Duration::from_secs(60), &AtomicBool::new(true));
    assert!(started.elapsed() < Duration::from_secs(5), "no wait");
    let status = child.wait().expect("reap");
    assert_eq!(status.signal(), Some(Signal::SIGKILL as i32));
}

#[test]
fn a_server_stop_does_not_wait_for_a_group_that_is_gone() {
    let (mut child, pid, _stdout) = group("echo ready; exec sleep 30");
    let jobs = registered(pid);
    terminate_all(&jobs);
    child.wait().expect("reap");
    let started = Instant::now();
    force_stopped(&jobs, Duration::from_secs(60), &AtomicBool::new(false));
    assert!(started.elapsed() < Duration::from_secs(5), "no wait");
    assert!(lock(&jobs).stopped_groups.is_empty());
}
