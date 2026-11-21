//! An adopted group's cleanup: the group is killed while its id still names
//! the group adopted, whether its leader runs, is an unreaped zombie, or was
//! reaped leaving members; a leader's pid held by another process, running or
//! a zombie, is never signalled. A stand-in group whose kill is refused while
//! its leader is a zombie fails cleanup when a live member remains. Each test
//! owns its leader through a `StandIn`, which kills the group while the
//! leader is unreaped; a member that outlives its leader's reap is also held
//! by an `Orphan`, so no process outlives the test.

use std::io::{BufRead, BufReader};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

use super::*;
use crate::process::process_is_zombie;
use crate::test_stand_in::{require_orphan_signals, retire, Reach, StandIn, POLL};

const BOUND: Duration = Duration::from_secs(5);

/// A sender that fails the test if cleanup signals anything.
fn never(_: Pid) -> nix::Result<()> {
    panic!("cleanup signalled a group id it no longer owns")
}

/// `sh` leading a process group of its own, with a `sleep 30` member, that
/// runs `then` once it printed the member's pid. Its stdin is a pipe.
fn leader_with_member(then: &str) -> (StandIn, u32) {
    let script = format!("sleep 30 & echo $!; {then}");
    let mut command = Command::new("sh");
    command
        .args(["-c", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut leader = StandIn::group_leader(&mut command);
    let stdout = leader.child().stdout.take().expect("stdout");
    let mut line = String::new();
    BufReader::new(stdout).read_line(&mut line).expect("read");
    let member = line.trim().parse().expect("the member's pid");
    (leader, member)
}

/// Wait at most [`BOUND`] for `pid` to be gone.
fn gone(pid: u32) -> Result<(), String> {
    let deadline = Instant::now() + BOUND;
    await_gone(pid, "process", deadline, BOUND, || !is_process_alive(pid))
}

/// Wait at most [`BOUND`] for `pid`, a child of this process, to be a zombie.
fn await_zombie(pid: u32) {
    let deadline = Instant::now() + BOUND;
    while !process_is_zombie(pid) {
        assert!(Instant::now() < deadline, "{pid} never exited");
        std::thread::sleep(POLL);
    }
}

#[test]
fn a_group_whose_leader_is_verified_alive_is_killed() {
    require_orphan_signals!();
    let (mut leader, member) = leader_with_member("wait");
    drop(AdoptedGroup::adopt(leader.id()));
    let status = leader.wait_within(BOUND);
    assert_eq!(status.signal(), Some(libc::SIGKILL), "{status:?}");
    assert_eq!(gone(member), Ok(()));
}

#[test]
fn a_group_whose_leader_exited_and_freed_its_pid_is_killed_through_its_members() {
    require_orphan_signals!();
    let (mut leader, member) = leader_with_member("read -r _; exit 0");
    let group = AdoptedGroup::adopt(leader.id());
    // `read` sees end of input, so the leader exits; reaping it frees its pid.
    drop(leader.child().stdin.take());
    assert!(leader.wait_within(BOUND).success());
    // With the leader reaped, only `group` would kill the member; the orphan
    // drops after the assertion, so it removes the member only if `group` failed.
    let _owner = Orphan::adopt(member);
    assert!(is_process_alive(member), "the member ended with its leader");
    drop(group);
    assert_eq!(gone(member), Ok(()));
}

/// EPERM is accepted from a group kill while the leader is a zombie, since
/// macOS answers so for a group of zombies; a live member that refused the
/// kill still fails cleanup within its bound.
#[test]
fn a_refused_group_kill_with_a_zombie_leader_and_a_live_member_fails_cleanup() {
    require_orphan_signals!();
    let (mut leader, member) = leader_with_member("exit 0");
    // Declared after `leader`, so it kills the member before the leader's drop.
    let _owner = Orphan::adopt(member);
    await_zombie(leader.id());
    let refuse = |_: Reach, _: Pid| Err(Errno::EPERM);
    let bound = Duration::from_millis(200);
    let started = Instant::now();
    let outcome = retire(leader.child(), Reach::Group, refuse, bound);
    let took = started.elapsed();
    let why = outcome.expect_err("a live member outlived the refused kill");
    assert!(why.contains("refused"), "{why}");
    assert!(took < bound + BOUND, "cleanup took {took:?}");
    assert!(is_process_alive(member), "the member was signalled");
}

/// An exited leader holds its pid and start time until its parent reaps it,
/// so the group it led is still the one adopted.
#[test]
fn a_group_whose_leader_is_an_unreaped_zombie_is_killed() {
    require_orphan_signals!();
    let (leader, member) = leader_with_member("exit 0");
    let pid = leader.id();
    await_zombie(pid);
    let start_time = process_start_time(pid);
    assert!(start_time.is_some(), "a zombie's start time is readable");
    drop(AdoptedGroup {
        leader: Some(ProcessIdentity { pid, start_time }),
    });
    assert_eq!(gone(member), Ok(()));
    drop(leader);
}

#[test]
fn a_leader_pid_held_by_another_running_process_is_not_signalled() {
    require_orphan_signals!();
    let stranger = StandIn::group_leader(Command::new("sleep").arg("30"));
    let pid = stranger.id();
    let start_time = process_start_time(pid).expect("a visible start time");
    let adopted = ProcessIdentity {
        pid,
        start_time: Some(start_time + 1),
    };
    assert_eq!(retire_group(adopted, never), Ok(()));
    assert!(is_process_alive(pid), "the stranger {pid} was signalled");
}

/// A zombie reads as dead to `is_process_alive`, yet still holds its pid.
#[test]
fn a_leader_pid_held_by_another_process_that_is_a_zombie_is_not_signalled() {
    require_orphan_signals!();
    let zombie = StandIn::group_leader(&mut Command::new("true"));
    let pid = zombie.id();
    await_zombie(pid);
    assert!(!is_process_alive(pid), "a zombie reads as dead");
    let start_time = process_start_time(pid).expect("a zombie's start time");
    let adopted = ProcessIdentity {
        pid,
        start_time: Some(start_time + 1),
    };
    assert_eq!(retire_group(adopted, never), Ok(()));
}
