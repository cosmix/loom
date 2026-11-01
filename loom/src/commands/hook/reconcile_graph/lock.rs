//! The `reconcile.lock` lease [`super::reconcile_graph`] and
//! [`super::spawn_if_needed`] share, in the context cache directory
//! (`crate::context::store::CACHE_RELATIVE_DIR`).
//!
//! ## Encoding
//!
//! One line, `"<epoch_secs> <pid> <failures> <pending> <claim>"`. Older
//! four-field lines parse with a zero claim. `pid != 0` means an
//! attempt STARTED at that epoch and is (as far as the lock says) still
//! running; `pid == 0` means an attempt FINISHED at that epoch. `0` is a safe
//! sentinel: never a real user process's pid on Linux or macOS. `failures` is
//! the count of consecutive failed runs (a success resets it) and drives the
//! backoff; `pending` is `1` when a request arrived while a live holder was
//! running, the bounded queue of one.
//!
//! A line that does not parse as exactly those four fields is treated as no
//! lock, which fails toward SPAWNING rather than toward permanent silence:
//! losing one debounce interval to a corrupt lock is merely annoying, treating
//! it as "someone is on it, forever" is a self-healing feature that stopped
//! healing.
//!
//! ## Writes
//!
//! Every write is a read-modify-write under the cache directory's exclusive
//! lock ([`update_lock`]) and lands through an atomic temp-file rename, so a
//! reader never sees the file missing, empty or half written, and no update is
//! lost: each write carries `failures` and `pending` forward from the line it
//! read. Nothing here ever unlinks the lock.
//!
//! ## Policy
//!
//! [`decide`] is the whole spawn policy, factored out from any process spawn
//! or lock write so it is testable without a real process:
//!
//! | Lock state                                          | Decision | Why |
//! | --------------------------------------------------- | -------- | --- |
//! | none, or unparseable                                | Spawn    | nothing to respect |
//! | `pid != 0`, alive, younger than `stale_lock_secs`   | Skip     | a run is genuinely in progress |
//! | `pid != 0`, alive, `stale_lock_secs` or older       | Spawn    | paranoia ceiling, independent of liveness |
//! | `pid != 0`, dead, any age                           | Spawn    | crashed; must not block healing for the ceiling's duration |
//! | `pid == 0`, younger than [`backoff_secs`]           | Skip     | throttled, doubling per failed run |
//! | `pid == 0`, [`backoff_secs`] or older               | Spawn    | throttle window elapsed |

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::context::store::ContextStore;
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::safe_read::read_to_string_bounded;

/// File name of the lease, inside the context cache directory.
const LOCK_FILE: &str = "reconcile.lock";

/// The most bytes `read_lock` reads: the line is four small integers, and the
/// directory it lives in is writable by a sandboxed session.
const MAX_LOCK_BYTES: usize = 256;

/// Failures beyond this many stop doubling the debounce.
const MAX_BACKOFF_DOUBLINGS: u32 = 5;

/// The longest a failing reconciler is ever throttled for.
const BACKOFF_CAP_SECS: u64 = 21_600;

/// Path of the lease, inside the context cache directory.
pub(super) fn reconcile_lock_path(store: &ContextStore) -> PathBuf {
    store.root().join(LOCK_FILE)
}

/// The parsed lock line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct LockState {
    pub epoch: u64,
    pub pid: u32,
    pub failures: u32,
    pub pending: bool,
    pub claim: u128,
}

impl LockState {
    fn parse(content: &str) -> Option<Self> {
        let mut parts = content.split_whitespace();
        let epoch = parts.next()?.parse().ok()?;
        let pid = parts.next()?.parse().ok()?;
        let failures = parts.next()?.parse().ok()?;
        let pending = match parts.next()? {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        let claim = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
        parts.next().is_none().then_some(LockState {
            epoch,
            pid,
            failures,
            pending,
            claim,
        })
    }

    fn render(&self) -> String {
        if self.claim == 0 {
            return format!(
                "{} {} {} {}\n",
                self.epoch,
                self.pid,
                self.failures,
                u8::from(self.pending)
            );
        }
        format!(
            "{} {} {} {} {}\n",
            self.epoch,
            self.pid,
            self.failures,
            u8::from(self.pending),
            self.claim
        )
    }
}

/// What [`decide`] concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LockDecision {
    /// No live, fresh lock in the way: safe to claim the lock and spawn.
    Spawn,
    /// A fresh lock belongs to a live reconcile, or a finished one is still
    /// within its backoff window: do nothing.
    Skip,
}

/// What a holder does after finishing a pass, see [`end_pass`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PassEnd {
    /// A request arrived meanwhile; the holder keeps the lease for one more pass.
    Again,
    /// The lease was released with the run's final failure count.
    Done,
    /// The line no longer names this holder (a takeover); nothing was written.
    Superseded,
}

/// Seconds a finished run stays throttled: `debounce_secs * 2^min(failures, 5)`,
/// capped at 21600 (never below `debounce_secs` itself).
pub(super) fn backoff_secs(debounce_secs: u64, failures: u32) -> u64 {
    let doublings = failures.min(MAX_BACKOFF_DOUBLINGS);
    debounce_secs
        .saturating_mul(1 << doublings)
        .min(BACKOFF_CAP_SECS.max(debounce_secs))
}

/// The spawn decision - see the module doc's policy table. Takes `now` and
/// `is_alive` as parameters rather than reading the clock or the process
/// table itself, so it is a pure function a test can drive deterministically
/// without a real process.
pub(super) fn decide(
    lock_path: &Path,
    now: u64,
    debounce_secs: u64,
    stale_lock_secs: u64,
    is_alive: impl Fn(u32) -> bool,
) -> LockDecision {
    let Some(state) = read_lock(lock_path) else {
        return LockDecision::Spawn;
    };
    let age = now.saturating_sub(state.epoch);

    if state.pid == 0 {
        // A finished marker: purely a throttle on how often a NEW attempt
        // may start, independent of any process's liveness.
        return if age < backoff_secs(debounce_secs, state.failures) {
            LockDecision::Skip
        } else {
            LockDecision::Spawn
        };
    }

    // An in-progress marker.
    if !is_alive(state.pid) {
        return LockDecision::Spawn; // crashed; take over regardless of age
    }
    if age >= stale_lock_secs {
        return LockDecision::Spawn; // hung; take over anyway
    }
    LockDecision::Skip
}

/// Parse the lock line from `lock_path`. `None` for a missing, unreadable, or
/// malformed file - all of which [`decide`] treats as "no lock" - and for one
/// that is not a plain small file: a symlink, a FIFO or an oversized file is
/// refused rather than followed or waited on.
///
/// Readers need no lock: every write is an atomic rename, so the file they
/// open is always a complete line.
pub(super) fn read_lock(lock_path: &Path) -> Option<LockState> {
    let dir = lock_path.parent()?;
    let name = lock_path.file_name()?;
    LockState::parse(&read_to_string_bounded(dir, Path::new(name), MAX_LOCK_BYTES).ok()?)
}

/// Read-modify-write the lock line under the cache directory's exclusive lock.
///
/// `modify` sees the current line (`None` when absent or unparseable) and
/// returns the line to write, or `None` to leave the file untouched, plus a
/// value handed back to the caller. `None` overall when the lock could not be
/// taken or written.
pub(super) fn update_lock<T>(
    lock_path: &Path,
    modify: impl FnOnce(Option<LockState>) -> (Option<LockState>, T),
) -> Option<T> {
    let dir = lock_path.parent()?;
    locked_dir_update(dir, || {
        let (next, out) = modify(read_lock(lock_path));
        if let Some(state) = next {
            atomic_write_locked(lock_path, &state.render())?;
        }
        Ok(out)
    })
    .ok()
}

/// `state` re-stamped as running under `pid` from `now`, carrying `failures`
/// and `pending` forward (a takeover of a crashed holder keeps its queue).
pub(super) fn held_by(state: Option<LockState>, now: u64, pid: u32) -> LockState {
    LockState {
        epoch: now,
        pid,
        claim: uuid::Uuid::new_v4().as_u128(),
        ..state.unwrap_or_default()
    }
}

/// The line a skipped request writes against a live `holder`: the holder's own
/// epoch, pid and counters with `pending` set, so it makes one more pass.
/// `None` when `pending` is already set - the queue holds one request, so
/// there is nothing to write. Runs inside the caller's [`update_lock`]
/// critical section, together with the [`decide`] that chose the skip.
pub(super) fn mark_pending(holder: LockState) -> Option<LockState> {
    (!holder.pending).then_some(LockState {
        pending: true,
        ..holder
    })
}

/// A spawned child takes over only the exact claim passed by its parent. A
/// direct invocation must first win the normal claim policy in its caller.
pub(super) fn begin_run(lock_path: &Path, pid: u32, claim: u128) -> bool {
    update_lock(lock_path, |state| match state {
        Some(held) if held.pid != 0 && held.claim == claim && claim != 0 => {
            (Some(LockState { pid, ..held }), true)
        }
        _ => (None, false),
    })
    .unwrap_or(false)
}

/// Hand the lease [`super::try_claim`] granted `from_pid` at `epoch` to the
/// spawned reconciler `to_pid`, so a second hook sees a live holder in the
/// window before the child's own [`begin_run`]. Nothing is written when the
/// line no longer carries that claim (the child already re-stamped it, or it
/// moved to another holder).
pub(super) fn rebind_claim(lock_path: &Path, epoch: u64, from_pid: u32, to_pid: u32, claim: u128) {
    update_lock(lock_path, |state| match state {
        Some(held) if held.pid == from_pid && held.epoch == epoch && held.claim == claim => (
            Some(LockState {
                pid: to_pid,
                ..held
            }),
            (),
        ),
        _ => (None, ()),
    });
}

/// After one pass, under the lock: keep the lease for another pass when a
/// request queued meanwhile, else release it. The holder keeps its pid through
/// every pass, so a concurrent [`super::try_claim`] never sees a released lease while
/// a pass is running. A failed pass counts one failure on top of the carried
/// count, a successful one resets it.
pub(super) fn end_pass(lock_path: &Path, now: u64, pid: u32, claim: u128, failed: bool) -> PassEnd {
    update_lock(lock_path, |state| match state {
        Some(held) if held.pid == pid && held.claim == claim => {
            let failures = if failed {
                held.failures.saturating_add(1)
            } else {
                0
            };
            let again = held.pending;
            let next = LockState {
                epoch: now,
                pid: if again { pid } else { 0 },
                failures,
                pending: false,
                claim: held.claim,
            };
            (
                Some(next),
                if again { PassEnd::Again } else { PassEnd::Done },
            )
        }
        _ => (None, PassEnd::Superseded),
    })
    .unwrap_or(PassEnd::Superseded)
}

/// Release the lease [`super::try_claim`] granted `pid` after the reconciler
/// could not be started: finished at `now`, one failure counted, `pending`
/// carried forward so a queued request is not lost. Nothing is written when the
/// line has since moved to another holder.
pub(super) fn release_failed(lock_path: &Path, now: u64, pid: u32, claim: u128) {
    update_lock(lock_path, |state| match state {
        Some(held) if held.pid == pid && held.claim == claim => (
            Some(LockState {
                epoch: now,
                pid: 0,
                failures: held.failures.saturating_add(1),
                pending: held.pending,
                claim: held.claim,
            }),
            (),
        ),
        _ => (None, ()),
    });
}

/// Record the run `pid` held as finished at `now` with `failures` unchanged,
/// unless the line has since moved to another holder. `--cancel` uses this.
pub(super) fn release_lease(lock_path: &Path, now: u64, pid: u32, claim: u128) {
    update_lock(lock_path, |state| match state {
        Some(held) if held.pid == pid && held.claim == claim => (
            Some(LockState {
                epoch: now,
                pid: 0,
                pending: false,
                ..held
            }),
            (),
        ),
        _ => (None, ()),
    });
}

/// Current unix time in whole seconds. `0` on a clock that reads before the
/// epoch (practically unreachable) rather than panicking - this runs on a
/// hook's spawn path, which must never disturb a session.
pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "tests_lock.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_read_lock.rs"]
mod tests_read_lock;

#[cfg(test)]
#[path = "tests_rebind.rs"]
mod tests_rebind;
