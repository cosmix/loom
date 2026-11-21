//! Contract tests for the shared process-group signalling and bounded wait helpers.
use nix::sys::signal::Signal;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Stdio;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use crate::test_stand_in::{require_orphan_signals, Orphan};
use crate::test_stand_in::{require_signals, StandIn};

/// True when `pid` no longer runs: its `/proc` entry is gone or it is a zombie.
#[cfg(target_os = "linux")]
fn gone_or_zombie(pid: u32) -> bool {
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
    require_orphan_signals!();
    let mut tree = StandIn::group_leader(
        Command::new("sh")
            .args(["-c", "sleep 30 & echo $!; wait"])
            .stdout(Stdio::piped()),
    );
    let child = tree.child();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let grandchild: u32 = line.trim().parse().unwrap();
    // Owned at once: once the leader is reaped below, dropping `tree` signals
    // no group, so this is what kills a grandchild that survived kill_group.
    let _grandchild = Orphan::adopt(grandchild);

    crate::process::kill_group(child);
    crate::process::wait_with_deadline(child, Duration::from_secs(5)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    while !gone_or_zombie(grandchild) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        gone_or_zombie(grandchild),
        "grandchild {grandchild} survived kill_group"
    );
}

#[test]
fn deadline_wait_returns_none() {
    require_signals!();
    let mut child = StandIn::direct(Command::new("sleep").arg("30"));
    let started = Instant::now();

    let outcome = crate::process::wait_with_deadline(child.child(), Duration::from_millis(100));
    let elapsed = started.elapsed();

    drop(child);
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
