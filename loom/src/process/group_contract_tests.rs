//! Contract tests for the shared process-group signalling and bounded wait helpers.
use nix::sys::signal::Signal;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// True when `pid` no longer runs: its `/proc` entry is gone or it is a zombie.
#[cfg(target_os = "linux")]
fn gone_or_zombie(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        // The state field follows the parenthesised command name, which may hold spaces.
        Ok(stat) => stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .is_some_and(|state| state == "Z"),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn group_kill_reaches_grandchild() {
    let mut child = Command::new("sh")
        .args(["-c", "sleep 30 & echo $!; wait"])
        .stdout(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let grandchild: i32 = line.trim().parse().unwrap();

    crate::process::kill_group(&mut child);
    crate::process::wait_with_deadline(&mut child, Duration::from_secs(5)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    while !gone_or_zombie(grandchild) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let gone = gone_or_zombie(grandchild);
    if !gone {
        // Do not leak the sleep past a failing run.
        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(grandchild), Signal::SIGKILL);
    }
    assert!(gone, "grandchild {grandchild} survived kill_group");
}

#[test]
fn deadline_wait_returns_none() {
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let started = Instant::now();

    let outcome = crate::process::wait_with_deadline(&mut child, Duration::from_millis(100));
    let elapsed = started.elapsed();

    let _ = child.kill();
    let _ = child.wait();
    assert!(
        outcome.unwrap().is_none(),
        "a running child reported an exit"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "wait ignored its limit: {elapsed:?}"
    );
}

#[test]
fn own_group_is_never_signalled() {
    let own = nix::unistd::getpgrp().as_raw();

    assert!(!crate::process::signal_group_id(own, Signal::SIGCONT));
    assert!(!crate::process::signal_group_id(1, Signal::SIGCONT));
    assert!(!crate::process::signal_group_id(0, Signal::SIGCONT));
}
