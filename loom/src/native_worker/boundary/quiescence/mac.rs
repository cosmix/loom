//! The macOS proof that a boundary left no process, and its bounded kill.
//!
//! Both judge a fresh census with `quiescence_scan::judge`. The read-only
//! proof ([`verify`]) counts every session, group or marker match. The kill
//! ([`kill_mac`]) signals the boundary's group only while its leader pins the
//! group number (the leader's recorded start time matches and it still leads
//! the session, or it is an unreaped zombie): a live or unreaped leader keeps
//! its pid, so no other session or group can take that number. Each later
//! round signals one process at a time, only members of that round's census
//! whose start time still matches immediately before the signal. Membership
//! learnt once stays with the (pid, start time) identity, which names exactly
//! one process, so a member that is reparented or drops its marker after a
//! round is still followed. One window remains: a member that exits and whose
//! pid is reused between the start-time check and the signal; pid reuse needs
//! the whole pid space to wrap in between.
use std::collections::HashSet;
use std::time::{Duration, Instant};

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use crate::native_worker::boundary::quiescence_scan::{
    self, BoundaryMark, Identity, Judgment, Observed, ProcEntry, Reading,
};
use crate::native_worker::boundary::Quiescence;
use crate::native_worker::ids::REAP_GRACE_SECS;
use crate::native_worker::model::MacIdentity;
use crate::process::macos_procs::{self, EnvRead, ProcRecord};
use crate::process::{process_is_zombie, process_start_time, signal_group_id};

/// The whole kill-and-scan, from the first signal to the verdict.
const KILL_BOUND: Duration = Duration::from_secs(30);
/// Between the group's `SIGTERM` and its `SIGKILL`.
const TERM_GRACE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(25);

/// Read-only: `Released` once a conclusive census within `REAP_GRACE_SECS`
/// finds no process of the boundary, else `Unverified` naming why.
pub(super) fn verify(identity: &MacIdentity) -> Quiescence {
    let deadline = Instant::now() + Duration::from_secs(REAP_GRACE_SECS);
    let mark = identity.mark();
    loop {
        let judgment = observe(&mark, &HashSet::new(), true);
        if judgment.is_empty() {
            return Quiescence::Released;
        }
        if Instant::now() >= deadline {
            return Quiescence::Unverified(judgment.detail(identity.session));
        }
        std::thread::sleep(POLL);
    }
}

/// Stops the boundary `identity` names and proves it stopped, within 30 s:
/// a changed boot id is `Released` at once; otherwise `SIGTERM` to the owned
/// group, `SIGKILL` to it after 5 s, then rounds of census and per-process
/// `SIGKILL` until a conclusive census is empty. `Unverified` names what was
/// left or uncertain at the bound. Nothing outside the matched set is ever
/// signalled.
pub(crate) fn kill_mac(identity: &MacIdentity) -> Quiescence {
    let deadline = Instant::now() + KILL_BOUND;
    match crate::process::boot_id::os_boot_id() {
        Err(error) => return Quiescence::Unverified(format!("cannot read the boot id: {error:#}")),
        Ok(boot_id) if boot_id != identity.boot_id => return Quiescence::Released,
        Ok(_) => {}
    }
    let mut sweep = Sweep::new(identity);
    if sweep.census() {
        return Quiescence::Released;
    }
    if sweep.signal_group(Signal::SIGTERM) {
        if sweep.wait_empty((Instant::now() + TERM_GRACE).min(deadline)) {
            return Quiescence::Released;
        }
        sweep.signal_group(Signal::SIGKILL);
    }
    loop {
        if sweep.census() {
            return Quiescence::Released;
        }
        if Instant::now() >= deadline {
            return Quiescence::Unverified(sweep.last.detail(identity.session));
        }
        sweep.kill_members();
        std::thread::sleep(POLL);
    }
}

/// The state of one kill: every member identity seen so far and the last
/// census's judgment.
struct Sweep<'a> {
    identity: &'a MacIdentity,
    mark: BoundaryMark,
    known: HashSet<Identity>,
    last: Judgment,
}

impl<'a> Sweep<'a> {
    fn new(identity: &'a MacIdentity) -> Self {
        Self {
            identity,
            mark: identity.mark(),
            known: HashSet::new(),
            last: Judgment::default(),
        }
    }

    /// One census; its members join `known`. True when it proves release.
    fn census(&mut self) -> bool {
        let numeric = leader_pins(self.identity);
        self.last = observe(&self.mark, &self.known, numeric);
        self.known.extend(self.last.members.iter().copied());
        self.last.is_empty()
    }

    /// Censuses until one proves release (true) or `until` passes (false).
    fn wait_empty(&mut self, until: Instant) -> bool {
        loop {
            if self.census() {
                return true;
            }
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(POLL);
        }
    }

    /// `killpg` of the boundary's group, only after rechecking that the leader
    /// still pins its number. False when it does not (nothing is sent). An
    /// `EPERM` (a group of zombies only) or `ESRCH` means nothing runs there.
    fn signal_group(&self, signal: Signal) -> bool {
        if !leader_pins(self.identity) {
            return false;
        }
        if let Ok(group) = i32::try_from(self.identity.session) {
            signal_group_id(group, signal);
        }
        true
    }

    /// `SIGKILL` to each member of the last census whose start time, read
    /// again just before the signal, is still the one the census showed.
    fn kill_members(&self) {
        for &(pid, start) in &self.last.members {
            if pid <= 1 || pid == std::process::id() {
                continue;
            }
            let Ok(raw) = i32::try_from(pid) else {
                continue;
            };
            if process_start_time(pid) == Some(start) {
                // A refusal or a vanished process shows in the next census.
                let _ = kill(Pid::from_raw(raw), Signal::SIGKILL);
            }
        }
    }
}

/// Whether the leader still holds its pid, so the session and group number
/// can name no other session or group: its start time is the recorded one and
/// it still leads that session, or it exited and is not yet reaped.
fn leader_pins(identity: &MacIdentity) -> bool {
    let Ok(recorded) = identity.leader_start.parse::<u64>() else {
        return false;
    };
    let Ok(raw) = libc::pid_t::try_from(identity.session) else {
        return false;
    };
    if raw <= 1 || process_start_time(identity.session) != Some(recorded) {
        return false;
    }
    // SAFETY: `getsid` reads one process attribute and touches no memory; the
    // pid is above 1, so it never names the caller implicitly.
    let sid = unsafe { libc::getsid(raw) };
    sid == raw || process_is_zombie(identity.session)
}

/// One census judged against `mark`.
fn observe(mark: &BoundaryMark, known: &HashSet<Identity>, numeric: bool) -> Judgment {
    let census = macos_procs::census(&mark.marker);
    // SAFETY: `getuid` and `geteuid` read the caller's credentials and cannot fail.
    let uids = unsafe { [libc::getuid(), libc::geteuid()] };
    let observed: Vec<Observed> = census
        .records
        .into_iter()
        .map(|record| observed(record, &uids))
        .collect();
    quiescence_scan::judge(
        &observed,
        census.incomplete.as_deref(),
        mark,
        known,
        numeric,
    )
}

fn observed(record: ProcRecord, uids: &[u32; 2]) -> Observed {
    let (reading, marker) = match record.env {
        EnvRead::Read(marker) => (Reading::Read, marker),
        EnvRead::Denied(why) => (Reading::Unreadable(why), None),
        EnvRead::Gone => (Reading::Gone, None),
    };
    Observed {
        entry: ProcEntry {
            pid: record.pid,
            ppid: record.ppid,
            pgid: record.pgid,
            // A zombie or vanished row (left out) or an unreadable one
            // (uncertain whatever its numbers) lacks one.
            sid: record.sid.unwrap_or(0),
            marker,
        },
        start: record.start,
        // The kernel refuses `KERN_PROCARGS2` by effective uid, so a setuid
        // process of this user is as unreadable as another user's. The row's
        // uid is compared with both our real and effective uid: a match with
        // either keeps it not foreign, so an unreadable row stays uncertain.
        foreign: !uids.contains(&record.uid),
        zombie: record.zombie,
        reading,
    }
}
