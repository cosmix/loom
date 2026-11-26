use super::*;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::sync::mpsc;
use std::thread;

/// A `sleep` leading its own process group: a group member that outlives the
/// collection deadline until the test kills it.
fn group_member() -> Child {
    Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .unwrap()
}

#[test]
fn a_group_member_still_exiting_extends_collection_past_the_deadline() {
    let mut member = group_member();
    let group = member.id();
    let (sender, receiver) = mpsc::channel();
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(600));
        member.kill().unwrap();
        member.wait().unwrap();
        sender.send("eof").unwrap();
    });
    let started = Instant::now();
    let mut deadline = CleanupDeadline::new(started + Duration::from_millis(100), group);
    assert_eq!(deadline.recv(&receiver), Ok("eof"));
    assert!(started.elapsed() >= Duration::from_millis(600));
    releaser.join().unwrap();
}

#[test]
fn a_group_seen_dying_gets_a_final_grace_once_it_empties() {
    let mut member = group_member();
    let group = member.id();
    let (sender, receiver) = mpsc::channel();
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        member.kill().unwrap();
        member.wait().unwrap();
        thread::sleep(Duration::from_millis(300));
        sender.send("eof").unwrap();
    });
    let started = Instant::now();
    let mut deadline = CleanupDeadline::new(started + Duration::from_millis(100), group);
    assert_eq!(deadline.recv(&receiver), Ok("eof"));
    releaser.join().unwrap();
}

#[test]
fn a_fixed_deadline_ends_collection_at_the_deadline() {
    let (_sender, receiver) = mpsc::channel::<()>();
    let started = Instant::now();
    let mut deadline = CleanupDeadline::fixed(started + Duration::from_millis(100));
    assert_eq!(deadline.recv(&receiver), Err(RecvTimeoutError::Timeout));
    assert!(started.elapsed() < Duration::from_millis(600));
}

#[test]
fn an_empty_group_ends_collection_at_the_deadline() {
    let mut gone = group_member();
    let group = gone.id();
    gone.kill().unwrap();
    gone.wait().unwrap();
    let (_sender, receiver) = mpsc::channel::<()>();
    let started = Instant::now();
    let mut deadline = CleanupDeadline::new(started + Duration::from_millis(100), group);
    assert_eq!(deadline.recv(&receiver), Err(RecvTimeoutError::Timeout));
    assert!(started.elapsed() < Duration::from_millis(600));
}

#[test]
fn a_group_that_never_empties_ends_collection_at_the_limit() {
    let mut member = group_member();
    let (_sender, receiver) = mpsc::channel::<()>();
    let started = Instant::now();
    let mut deadline = CleanupDeadline::new(started + Duration::from_millis(50), member.id());
    deadline.limit = started + Duration::from_millis(400);
    assert_eq!(deadline.recv(&receiver), Err(RecvTimeoutError::Timeout));
    let elapsed = started.elapsed();
    member.kill().unwrap();
    member.wait().unwrap();
    assert!(elapsed >= Duration::from_millis(400), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

#[test]
fn the_callers_own_group_never_extends_collection() {
    let own = nix::unistd::getpgrp().as_raw().unsigned_abs();
    let (_sender, receiver) = mpsc::channel::<()>();
    let started = Instant::now();
    let mut deadline = CleanupDeadline::new(started + Duration::from_millis(50), own);
    assert_eq!(deadline.recv(&receiver), Err(RecvTimeoutError::Timeout));
    assert!(started.elapsed() < Duration::from_millis(500));
}
