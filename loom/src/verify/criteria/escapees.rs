//! At a criterion's timeout, kill every process the criterion started,
//! including those that left its process group, and none it did not start.
//!
//! First the leader's process group (the anchor's, under one) is stopped
//! with `SIGSTOP`: a stopped leader runs no further command of a list and
//! respawns no worker while the sweep runs, and keeps its children, so none
//! is reparented before it is found. The owners then kill and reap the
//! stopped leader and anchor (`OwnedProcess::retire`, `GroupAnchor::retire`).
//!
//! Membership is read from the process table while the id that names it
//! cannot be reused:
//! - Without a group anchor the leader leads its own session
//!   ([`lead_own_session`]). The session id is the leader's pid, which the
//!   unreaped leader holds even as a zombie the table cannot show, so every
//!   process in that session is the criterion's.
//! - Under a host job's anchor the leader joins the anchor's group, in the
//!   session the job's child leads (the runner starts it with
//!   [`lead_own_session`]). That session also holds what the child's earlier
//!   criteria left behind, so a member of it is the criterion's when its
//!   parent chain leads back to the child through the leader or the anchor,
//!   or when the chain breaks (at an orphan, or at a process whose parent left
//!   the session) at a process that started no earlier than the leader. A
//!   fixture server an earlier criterion left started before this leader and
//!   is left alone. Linux counts start times in clock ticks (10 ms), so an
//!   orphan forked in the leader's own tick counts as the criterion's.
//! - A child of a member, and a member of a group a member leads, are the
//!   criterion's too. This reaches a descendant that started a session of its
//!   own while its parent still runs.
//!
//! Limit: a descendant that starts a session of its own and whose parent exits
//! before the timeout is not found where nothing contains the criterion's
//! descendants: on macOS, which gives a user process no pid namespace, cgroup
//! or child subreaper, and on Linux for a command whose confinement is
//! `inherit`. A confined Linux command runs in a pid namespace of its own,
//! which the kernel empties when bubblewrap dies.

use std::collections::{BTreeSet, HashMap};
use std::process::Command;

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use super::process_table::{self, ProcessEntry};
use crate::process::{
    process_start_time, signal_group_id, verify_process_identity, IdentityStatus, ProcessIdentity,
};

/// Table reads per timeout: a process forking while the first round kills
/// leaves children that the next round finds.
const ROUNDS: usize = 4;

/// A start time by pid ([`process_start_time`]), `None` when unreadable.
type StartTime<'a> = &'a dyn Fn(u32) -> Option<u64>;

/// Make `command`'s process lead a new session, and so a new process group.
pub(crate) fn lead_own_session(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: the closure runs between fork and exec and calls only `setsid`,
    // which is async-signal-safe, and reads `errno`; it allocates nothing.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// A criterion's leader as it was spawned.
#[derive(Debug, Clone, Copy)]
pub(super) struct Leader {
    pub(super) pid: u32,
    /// The pid of the group anchor whose group the leader joined, if any.
    pub(super) anchor: Option<u32>,
    /// The leader's start time, read as it spawned: a zombie's may not be
    /// readable later. `None` counts no session orphan as the criterion's.
    pub(super) started: Option<u64>,
}

impl Leader {
    /// The leader `pid`, just spawned into `anchor`'s group if one is given.
    pub(super) fn spawned(pid: u32, anchor: Option<u32>) -> Self {
        Self {
            pid,
            anchor,
            started: process_start_time(pid),
        }
    }
}

/// Stop the criterion's group, then SIGKILL every process the criterion
/// started except the leader and the anchor, which their owners kill and
/// reap. The leader must not be reaped yet.
pub(super) fn kill_escapees(leader: Leader) {
    // The group id is pinned by its unreaped leader or anchor.
    if let Ok(group) = i32::try_from(leader.anchor.unwrap_or(leader.pid)) {
        signal_group_id(group, Signal::SIGSTOP);
    }
    let me = std::process::id();
    let mut signalled = BTreeSet::new();
    for _ in 0..ROUNDS {
        let table = process_table::snapshot();
        let fresh: Vec<ProcessIdentity> = members(&table, leader, me, &process_start_time)
            .into_iter()
            .filter(|entry| signalled.insert(entry.pid))
            .filter_map(identity)
            .collect();
        if fresh.is_empty() {
            return;
        }
        fresh.into_iter().for_each(kill_verified);
    }
}

/// `member`'s pid and start time, when the pid still shows the member's
/// group and session once the start time is read. Every identity is taken
/// before any kill: a kill reparents the victim's children, so their parent
/// field changes, while group and session do not.
fn identity(member: ProcessEntry) -> Option<ProcessIdentity> {
    let start_time = process_start_time(member.pid)?;
    let now = process_table::entry(member.pid)?;
    (now.pgid == member.pgid && now.sid == member.sid).then_some(ProcessIdentity {
        pid: member.pid,
        start_time: Some(start_time),
    })
}

/// SIGKILL the process `identity` names if its start time still holds.
fn kill_verified(identity: ProcessIdentity) {
    if verify_process_identity(identity) != IdentityStatus::VerifiedAlive {
        return;
    }
    if let Ok(pid) = i32::try_from(identity.pid) {
        let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
    }
}

/// The processes of `table` the criterion `leader` leads started, `me`, the
/// leader and its anchor left out.
fn members(
    table: &[ProcessEntry],
    leader: Leader,
    me: u32,
    start_time: StartTime<'_>,
) -> Vec<ProcessEntry> {
    let by_pid: HashMap<u32, ProcessEntry> =
        table.iter().map(|entry| (entry.pid, *entry)).collect();
    let session = match leader.anchor {
        None => Some(leader.pid),
        Some(_) => by_pid
            .get(&me)
            .filter(|entry| entry.sid == me)
            .map(|entry| entry.sid),
    };
    let group = leader.anchor.unwrap_or(leader.pid);
    let ours = |entry: &&ProcessEntry| {
        entry.pid == leader.pid
            || entry.pgid == group
            || session.is_some_and(|session| {
                entry.sid == session
                    && session_member_is_ours(entry, &by_pid, me, leader, start_time)
            })
    };
    let mut found: BTreeSet<u32> = table
        .iter()
        .filter(|entry| entry.pid != me)
        .filter(ours)
        .map(|entry| entry.pid)
        .collect();
    loop {
        let before = found.len();
        for entry in table.iter().filter(|entry| entry.pid != me) {
            if found.contains(&entry.ppid) || found.contains(&entry.pgid) {
                found.insert(entry.pid);
            }
        }
        if found.len() == before {
            break;
        }
    }
    let excluded = [Some(me), Some(leader.pid), leader.anchor];
    table
        .iter()
        .filter(|entry| found.contains(&entry.pid) && !excluded.contains(&Some(entry.pid)))
        .copied()
        .collect()
}

/// Whether a member of the session `me` leads is the criterion's. In the
/// leader's own session (`me` is not its leader) every member is. In `me`'s
/// session a member is the criterion's when its parent chain reaches `me`
/// through the leader or the anchor, or breaks at a process that started no
/// earlier than the leader.
fn session_member_is_ours(
    entry: &ProcessEntry,
    by_pid: &HashMap<u32, ProcessEntry>,
    me: u32,
    leader: Leader,
    start_time: StartTime<'_>,
) -> bool {
    if entry.sid != me {
        return true;
    }
    let mut current = *entry;
    for _ in 0..by_pid.len() {
        if current.ppid == me {
            return current.pid == leader.pid || Some(current.pid) == leader.anchor;
        }
        match by_pid.get(&current.ppid) {
            Some(parent) if parent.sid == me => current = *parent,
            // Its parent left the session or exited: it was orphaned.
            _ => {
                return leader
                    .started
                    .zip(start_time(current.pid))
                    .is_some_and(|(leader, orphan)| orphan >= leader)
            }
        }
    }
    false
}

#[cfg(test)]
#[path = "escapees_tests.rs"]
mod tests;
