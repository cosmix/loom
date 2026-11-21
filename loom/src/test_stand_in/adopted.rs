//! Processes a test does not reap: an orphan reparented away from it, and a
//! process group the code under test started. Each is identified by its pid
//! and start time when it is taken over, and signalled only while that start
//! time still matches or, for a group whose leader exited, while no process
//! holds the leader's pid. With no start time to compare, cleanup reports the
//! process and leaves it alone.

use std::ffi::OsStr;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{kill, killpg, Signal};
use nix::unistd::Pid;

use super::{await_gone, refused, settle, target_of, test_name, CLEANUP_BOUND};
use crate::process::sandbox_probe::{orphans_signalable, spawn_orphan};
use crate::process::{
    is_process_alive, process_start_time, verify_process_identity, IdentityStatus, ProcessIdentity,
};

/// A process reparented away from the test, so a dead one is reaped by init
/// and stops answering `kill(pid, 0)`, as a session's agent does once loom
/// retires it. Killed on drop if it is still the process that was started.
pub(crate) struct Orphan {
    pid: u32,
    /// `None` when the process was already gone at adoption: there is nothing
    /// to kill, and the pid may come to name another process.
    pub(super) identity: Option<ProcessIdentity>,
}

/// An orphan that sleeps for 30 seconds.
pub(crate) fn orphan() -> Orphan {
    orphan_running("sleep 30", &[])
}

/// An orphan running the shell command `body`, with `args` as its `$1`, `$2`
/// and on. `sh` backgrounds only the last command of a `;` list in `body`
/// (the earlier ones run first, in the foreground), so the orphan is that
/// last command; group a list with `( ... )` to background all of it.
pub(crate) fn orphan_running(body: &str, args: &[&OsStr]) -> Orphan {
    assert!(
        orphans_signalable(),
        "{}: an orphan was spawned without require_orphan_signals!() first",
        test_name()
    );
    let pid = spawn_orphan(body, args).unwrap_or_else(|error| panic!("spawn orphan: {error}"));
    Orphan::adopt(pid)
}

/// What a pid read as when it was taken over: `None` once it was gone.
fn identity_now(pid: u32) -> Option<ProcessIdentity> {
    let start_time = process_start_time(pid);
    is_process_alive(pid).then_some(ProcessIdentity { pid, start_time })
}

/// The failure for a process whose start time cannot be compared.
fn unverifiable(pid: u32, noun: &str) -> String {
    format!(
        "stand-in {pid}: no start time shows that its {noun} is still the one the test \
         started; not signalled, so it may still be running"
    )
}

impl Orphan {
    /// Take over `pid`, a process reparented away from the test, so dropping
    /// the result kills it as if the test had started it. Its identity is the
    /// process running now, so adopt it as soon as the test learns the pid:
    /// a pid adopted after its process ended may name another process.
    pub(crate) fn adopt(pid: u32) -> Self {
        Orphan {
            pid,
            identity: identity_now(pid),
        }
    }

    /// Take over the process `identity` names, as the test already read it.
    pub(crate) fn from_identity(identity: ProcessIdentity) -> Self {
        Orphan {
            pid: identity.pid,
            identity: Some(identity),
        }
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }
}

impl Drop for Orphan {
    fn drop(&mut self) {
        let Some(identity) = self.identity else {
            return;
        };
        let send = |target| kill(target, Signal::SIGKILL);
        settle(retire_orphan(identity, send, CLEANUP_BOUND));
    }
}

/// Kill the orphan `identity` names through `send` while its start time
/// still matches, then wait at most `bound` until it is gone.
pub(super) fn retire_orphan(
    identity: ProcessIdentity,
    send: impl Fn(Pid) -> nix::Result<()>,
    bound: Duration,
) -> Result<(), String> {
    let pid = identity.pid;
    match verify_process_identity(identity) {
        IdentityStatus::Dead => return Ok(()),
        IdentityStatus::Unverifiable => return Err(unverifiable(pid, "process")),
        IdentityStatus::VerifiedAlive => {}
    }
    refused(pid, "process", send(target_of(pid)))?;
    let deadline = Instant::now() + bound;
    await_gone(pid, "process", deadline, bound, || {
        verify_process_identity(identity) != IdentityStatus::VerifiedAlive
    })
}

/// A process group the code under test started, killed with `SIGKILL` on drop
/// when its members outlive the test. The test reaps none of them, so cleanup
/// checks the kill and does not wait: an unreaped member of a group the test
/// does not own would read as a survivor however promptly it died. Cleanup
/// compares the leader's start time, so a test that adopts one starts with
/// `require_orphan_signals!()`, whose probe reads a start time.
pub(crate) struct AdoptedGroup {
    /// `None` when the leader was already gone at adoption: its group id may
    /// come to name another group.
    pub(super) leader: Option<ProcessIdentity>,
}

impl AdoptedGroup {
    /// Take over the group `pgid` leads while its leader is running.
    pub(crate) fn adopt(pgid: u32) -> Self {
        AdoptedGroup {
            leader: identity_now(pgid),
        }
    }
}

impl Drop for AdoptedGroup {
    fn drop(&mut self) {
        let Some(leader) = self.leader else {
            return;
        };
        let send = |target| killpg(target, Signal::SIGKILL);
        settle(retire_group(leader, send));
    }
}

/// Kill, through `send`, the group `leader` led while its id is still the
/// one adopted.
pub(super) fn retire_group(
    leader: ProcessIdentity,
    send: impl Fn(Pid) -> nix::Result<()>,
) -> Result<(), String> {
    if !still_adopted(leader)? {
        return Ok(());
    }
    refused(leader.pid, "process group", send(target_of(leader.pid)))
}

/// Whether the group id `leader` held still names the group adopted, read
/// from what holds the leader's pid now. A zombie keeps its pid and start
/// time until its parent reaps it, so it is compared as a running process is.
fn still_adopted(leader: ProcessIdentity) -> Result<bool, String> {
    let pgid = leader.pid;
    match (leader.start_time, process_start_time(pgid)) {
        // The leader runs, or exited and is not reaped yet.
        (Some(recorded), Some(now)) if recorded == now => Ok(true),
        // Another process, running or a zombie, holds the leader's pid: it
        // was given that pid after the group emptied, so the id is no
        // longer ours.
        (Some(_), Some(_)) => Ok(false),
        // No process holds the pid: a group id stays reserved while any
        // member remains, so a group answering it is the one adopted,
        // provided the adopted group has never emptied since adoption.
        (_, None) if kill(target_of(pgid), None) == Err(Errno::ESRCH) => Ok(true),
        _ => Err(unverifiable(pgid, "process group")),
    }
}

#[cfg(test)]
#[path = "adopted_tests.rs"]
mod tests;
