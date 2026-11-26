//! When a retired operation stops waiting for its output pipes.
//!
//! A pipe reaches end of file once its last writer releases it, and a process
//! releases its descriptors only at the end of its own exit, after the kernel
//! has torn down any namespace it was the last member of. A killed member of
//! the operation's group can therefore hold a pipe after the leader was reaped;
//! collection waits for such a member, and the plain [`CLEANUP_GRACE`] bounds
//! only holders outside the group.
//!
//! A confined Linux command has such a holder outside every group the caller
//! can name: bubblewrap's outer process (the leader) exits once the sandbox's
//! pid 1 reports the command's status, while that pid 1, which keeps the
//! command's stdio, is still releasing the sandbox's namespaces. Under
//! `--new-session` it leads a session of its own, so no group poll sees it.
//! Every process left in the sandbox dies with that pid 1 (`--die-with-parent`
//! kills it when the leader exits), so collection may wait for end of file up
//! to the same limit a dying group gets ([`CleanupDeadline::contained`]).

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::owned::CLEANUP_GRACE;

/// How long past the retirement deadline collection may wait for a killed
/// group or a sandbox to finish exiting. Revisit if a namespace or
/// address-space teardown on a loaded host is seen to take longer.
const DYING_GROUP_LIMIT: Duration = Duration::from_secs(10);
const GROUP_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy)]
pub(crate) struct CleanupDeadline {
    at: Instant,
    /// The killed group, until it is seen empty or the limit passes.
    group: Option<i32>,
    limit: Instant,
    /// Whether the group was seen with a member once `at` had passed.
    dying: bool,
    seen: Seen,
}

/// What the deadline last knew of the processes it waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    /// No group tracked: a fixed deadline, or a group not yet polled.
    Untracked,
    /// A command whose processes live in a pid namespace of their own.
    Contained,
    Member,
    Empty,
}

impl CleanupDeadline {
    /// Collection ends at `at`, the deadline `OwnedProcess::retire` returned,
    /// or later while process group `group`, already killed, has a member.
    pub(crate) fn new(at: Instant, group: u32) -> Self {
        Self {
            at,
            group: i32::try_from(group).ok(),
            limit: at + DYING_GROUP_LIMIT,
            dying: false,
            seen: Seen::Untracked,
        }
    }

    /// Collection ends at `at`, with no group to extend it: for an operation
    /// that timed out, whose output is already judged incomplete.
    pub(crate) fn fixed(at: Instant) -> Self {
        Self {
            at,
            group: None,
            limit: at,
            dying: false,
            seen: Seen::Untracked,
        }
    }

    /// Collection ends at end of file or [`DYING_GROUP_LIMIT`] past `at`: for
    /// a finished command whose every process lives in a pid namespace that
    /// the kernel empties once the leader is gone (see the module docs).
    pub(crate) fn contained(at: Instant) -> Self {
        let limit = at + DYING_GROUP_LIMIT;
        Self {
            at: limit,
            group: None,
            limit,
            dying: false,
            seen: Seen::Contained,
        }
    }

    /// Receive from `receiver` by this deadline, extending it as [`Self::new`] says.
    pub(crate) fn recv<T>(&mut self, receiver: &Receiver<T>) -> Result<T, RecvTimeoutError> {
        loop {
            let remaining = self.at.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(remaining) {
                Err(RecvTimeoutError::Timeout) if self.extend() => {}
                received => return received,
            }
        }
    }

    /// What collection waited for, for a timeout's diagnosis: empty, or a
    /// clause starting with `, `.
    pub(crate) fn describe(&self) -> &'static str {
        match self.seen {
            Seen::Untracked => "",
            Seen::Contained => ", sandboxed",
            Seen::Member => ", group still had a member",
            Seen::Empty => ", group empty",
        }
    }

    /// Move `at` on once it has passed: a poll interval while the group has a
    /// member, then one [`CLEANUP_GRACE`] for holders outside the group once
    /// the group, seen dying, is empty. `false` when collection should stop.
    fn extend(&mut self) -> bool {
        let Some(group) = self.group else {
            return false;
        };
        let member = group_has_member(group);
        self.seen = if member { Seen::Member } else { Seen::Empty };
        let now = Instant::now();
        if now >= self.limit {
            self.group = None;
            return false;
        }
        if member {
            self.dying = true;
            self.at = (now + GROUP_POLL_INTERVAL).min(self.limit);
            return true;
        }
        self.group = None;
        if !self.dying {
            return false;
        }
        self.at = (now + CLEANUP_GRACE).min(self.limit);
        true
    }
}

/// Whether process group `group` still has a member, zombies included. A group
/// `<= 1` or the caller's own counts as empty, as `signal_group_id` refuses them.
/// Once the leader is reaped, an unrelated group could reuse the id; no signal
/// is sent, and [`DYING_GROUP_LIMIT`] bounds the cost.
fn group_has_member(group: i32) -> bool {
    use nix::errno::Errno;
    if group <= 1 || group == nix::unistd::getpgrp().as_raw() {
        return false;
    }
    match nix::sys::signal::killpg(nix::unistd::Pid::from_raw(group), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "cleanup_deadline_tests.rs"]
mod tests;
