//! Which processes of a process table belong to a macOS boundary.
//!
//! A macOS boundary has no pid namespace. Its leader starts a fresh session
//! (`setsid`), so the leader's pid is also the session id and the process
//! group id, and every process it starts inherits the exact environment entry
//! `LOOM_NATIVE_BOUNDARY=<invocation id>`. A process belongs to the boundary
//! when it keeps that session, that group or that marker, or descends from a
//! process that does.
//!
//! Known limitation (plan Decisions 9): a detached process that called
//! `setsid`, was reparented away from every matched ancestor and overwrote
//! the marker in its initial environment (the area `KERN_PROCARGS2` reads)
//! carries no evidence the scan can see, so it escapes the scan. It still
//! cannot leave the Seatbelt profile: it stays confined to the boundary's
//! writable roots. A process that also changed its user by a setuid exec
//! looks like another user's process, whose unreadable environment the
//! census ignores (see `judge`).
//!
//! Everything here is pure and platform independent; the macOS census that
//! feeds it lives in `crate::process::macos_procs`.
use std::collections::BTreeSet;
#[cfg(any(target_os = "macos", test))]
use std::collections::HashSet;

/// Name of the environment entry every boundary process inherits.
pub const BOUNDARY_MARKER_VAR: &str = "LOOM_NATIVE_BOUNDARY";

/// One process as a census showed it. `marker` holds the exact `NAME=value`
/// environment entry named [`BOUNDARY_MARKER_VAR`], or `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcEntry {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    pub sid: u32,
    pub marker: Option<String>,
}

/// What identifies a boundary's processes: its session, its group and its
/// exact marker entry (`NAME=value`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryMark {
    pub sid: u32,
    pub pgid: u32,
    pub marker: String,
}

/// The pids of `table` that match `mark` by session, group or exact marker,
/// plus their transitive descendants; sorted and deduplicated.
pub fn survivors(table: &[ProcEntry], mark: &BoundaryMark) -> Vec<u32> {
    closure(table, |entry| {
        entry.sid == mark.sid || entry.pgid == mark.pgid || marked(entry, mark)
    })
}

fn marked(entry: &ProcEntry, mark: &BoundaryMark) -> bool {
    entry.marker.as_deref() == Some(mark.marker.as_str())
}

/// The pids `seed` accepts plus every transitive descendant (by `ppid`),
/// sorted and deduplicated. Iterates to a fixed point, so the table's order
/// does not matter.
fn closure(table: &[ProcEntry], seed: impl Fn(&ProcEntry) -> bool) -> Vec<u32> {
    let mut set: BTreeSet<u32> = table.iter().filter(|e| seed(e)).map(|e| e.pid).collect();
    loop {
        let before = set.len();
        for entry in table {
            if entry.pid != entry.ppid && set.contains(&entry.ppid) {
                set.insert(entry.pid);
            }
        }
        if set.len() == before {
            return set.into_iter().collect();
        }
    }
}

#[cfg(any(target_os = "macos", test))]
/// How much of one process beyond its table row the census could read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reading {
    /// Session and environment were read; `entry.marker` is authoritative.
    Read,
    /// The process exists but its environment could not be read (the text says
    /// why): it may carry the marker.
    Unreadable(String),
    /// The process exited between the table read and its per-process read.
    Gone,
}

#[cfg(any(target_os = "macos", test))]
/// One census row: the entry plus what decides whether it can be trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Observed {
    pub(crate) entry: ProcEntry,
    /// Kernel start time (`process::process_start_time` units); with the pid it
    /// names exactly one process.
    pub(crate) start: u64,
    /// Its uid is neither this process's real nor its effective uid: the
    /// kernel refuses its environment by design (another user's process, or a
    /// setuid exec). A uid matching either one is not foreign, so an
    /// unreadable process of this user stays uncertain.
    pub(crate) foreign: bool,
    /// Exited, not yet reaped: it runs nothing and holds no descriptor.
    pub(crate) zombie: bool,
    pub(crate) reading: Reading,
}

#[cfg(any(target_os = "macos", test))]
/// A process identity that cannot be reused: pid plus start time.
pub(crate) type Identity = (u32, u64);

#[cfg(any(target_os = "macos", test))]
/// The boundary's live members of one census, and why it cannot prove more.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Judgment {
    /// Members whose membership is proven, sorted by pid.
    pub(crate) members: Vec<Identity>,
    /// Reasons membership is uncertain; empty when the census is conclusive.
    pub(crate) uncertain: Vec<String>,
}

#[cfg(any(target_os = "macos", test))]
impl Judgment {
    /// Proof of release: no member and nothing uncertain.
    pub(crate) fn is_empty(&self) -> bool {
        self.members.is_empty() && self.uncertain.is_empty()
    }

    /// The reason release is not proven, naming at most five pids per list.
    pub(crate) fn detail(&self, session: u32) -> String {
        let mut parts = Vec::new();
        if !self.members.is_empty() {
            let pids: Vec<String> = self
                .members
                .iter()
                .take(5)
                .map(|m| m.0.to_string())
                .collect();
            parts.push(format!(
                "{} process(es) of boundary session {session} still run: pids {}",
                self.members.len(),
                pids.join(", ")
            ));
        }
        let more = self.uncertain.len().saturating_sub(5);
        parts.extend(self.uncertain.iter().take(5).cloned());
        if more > 0 {
            parts.push(format!("{more} more uncertain process(es)"));
        }
        parts.join("; ")
    }
}

#[cfg(any(target_os = "macos", test))]
/// Judges one census against `mark`.
///
/// Members are the [`survivors`] of the live table (seeded by the session or
/// group number only when `numeric` is true, else by the marker alone) plus
/// the processes with an identity in `known` and their descendants. `numeric` may be true only while something pins
/// that number to this boundary (its leader is alive or an unreaped zombie);
/// otherwise a number match is reported as uncertain instead. Zombies are
/// left out: they run nothing. Membership is uncertain for `incomplete`, for
/// an unreadable process of this user that nothing already places, and for a
/// child whose parent exited mid-census. An unreadable process of another
/// user that no session, group or ancestry match places is ignored: the
/// boundary runs as this user and reaches another user only by a setuid exec.
pub(crate) fn judge(
    observed: &[Observed],
    incomplete: Option<&str>,
    mark: &BoundaryMark,
    known: &HashSet<Identity>,
    numeric: bool,
) -> Judgment {
    let live: Vec<&Observed> = observed
        .iter()
        .filter(|o| !o.zombie && o.reading != Reading::Gone)
        .collect();
    let table: Vec<ProcEntry> = live.iter().map(|o| o.entry.clone()).collect();
    let by_number = |e: &ProcEntry| e.sid == mark.sid || e.pgid == mark.pgid;
    let known_pids: HashSet<u32> = live
        .iter()
        .filter(|o| known.contains(&(o.entry.pid, o.start)))
        .map(|o| o.entry.pid)
        .collect();
    let seed = if numeric {
        mark.clone()
    } else {
        marker_only(mark, &table)
    };
    let mut pids: BTreeSet<u32> = survivors(&table, &seed).into_iter().collect();
    pids.extend(closure(&table, |e| known_pids.contains(&e.pid)));
    let pids: Vec<u32> = pids.into_iter().collect();
    let members: Vec<Identity> = pids
        .iter()
        .filter_map(|pid| live.iter().find(|o| o.entry.pid == *pid))
        .map(|o| (o.entry.pid, o.start))
        .collect();
    let mut uncertain: Vec<String> = incomplete.map(str::to_string).into_iter().collect();
    let placed = |pid: u32| pids.binary_search(&pid).is_ok();
    for o in &live {
        let pid = o.entry.pid;
        if placed(pid) {
            continue;
        }
        if by_number(&o.entry) {
            uncertain.push(format!(
                "pid {pid} has the boundary's session or group number, but no live leader \
                 pins that number to the boundary"
            ));
        } else if let (Reading::Unreadable(why), false) = (&o.reading, o.foreign) {
            uncertain.push(format!("pid {pid} cannot be placed: {why}"));
        }
    }
    uncertain.extend(orphaned(observed, &live, &placed));
    Judgment { members, uncertain }
}

#[cfg(any(target_os = "macos", test))]
/// `mark` with a session and group number no entry of `table` has, so
/// [`survivors`] matches by the marker alone.
fn marker_only(mark: &BoundaryMark, table: &[ProcEntry]) -> BoundaryMark {
    let used: HashSet<u32> = table.iter().flat_map(|e| [e.sid, e.pgid]).collect();
    // A table of n entries uses at most 2n numbers, so one of 0..=2n is free.
    let free = (0..=u32::MAX)
        .find(|n| !used.contains(n))
        .unwrap_or(u32::MAX);
    BoundaryMark {
        sid: free,
        pgid: free,
        marker: mark.marker.clone(),
    }
}

#[cfg(any(target_os = "macos", test))]
/// Children whose parent exited before its row could be read: the parent's
/// membership, and so theirs by ancestry, is unknown.
fn orphaned(
    observed: &[Observed],
    live: &[&Observed],
    placed: &dyn Fn(u32) -> bool,
) -> Vec<String> {
    let gone: HashSet<u32> = observed
        .iter()
        .filter(|o| o.reading == Reading::Gone)
        .map(|o| o.entry.pid)
        .collect();
    live.iter()
        .filter(|o| gone.contains(&o.entry.ppid) && !placed(o.entry.pid))
        .map(|o| {
            format!(
                "pid {} exited during the census, so whether its child {} belongs is unknown",
                o.entry.ppid, o.entry.pid
            )
        })
        .collect()
}
