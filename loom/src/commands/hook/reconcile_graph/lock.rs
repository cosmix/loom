//! The `reconcile.lock` debounce file [`super::reconcile_graph`] and
//! [`super::spawn_if_needed`] share, in the context cache directory
//! (`crate::context::store::CACHE_RELATIVE_DIR`).
//!
//! ## Encoding
//!
//! One line, `"<epoch_secs> <pid>"`. `pid` doubles as a marker: `pid != 0`
//! means an attempt STARTED at that epoch and is (as far as the lock says)
//! still running; `pid == 0` means an attempt FINISHED at that epoch. `0` is
//! a safe sentinel — never a real user process's pid on Linux or macOS — and
//! it is chosen so a genuinely corrupt lock (unparseable epoch or pid) fails
//! toward SPAWNING rather than toward permanent silence: [`read_lock`] treats
//! a parse failure identically to a missing file, which is always eligible
//! for a fresh claim. Losing one debounce interval to a corrupt lock is a
//! bug that is merely annoying; treating a corrupt lock as "someone is on
//! it, forever" is a self-healing feature that stopped healing.
//!
//! ## Policy
//!
//! [`decide`] is the whole thing, factored out from any process spawn or
//! lock write so it is testable without touching a real process (see
//! `tests_lock.rs`):
//!
//! | Lock state                                   | Decision | Why |
//! | --------------------------------------------- | -------- | --- |
//! | none, or unparseable                          | Spawn    | nothing to respect |
//! | `pid != 0`, alive, younger than `stale_lock_secs` | Skip | a run is genuinely in progress |
//! | `pid != 0`, alive, `stale_lock_secs` or older | Spawn    | paranoia ceiling, independent of liveness |
//! | `pid != 0`, dead, any age                     | Spawn    | crashed; must not block healing for the ceiling's duration |
//! | `pid == 0`, younger than `debounce_secs`      | Skip     | throttled — an attempt only just finished |
//! | `pid == 0`, `debounce_secs` or older           | Spawn    | throttle window elapsed |
//!
//! Nothing here ever unlinks the lock: [`claim_lock`] rewrites it, on
//! [`super::try_reconcile`]'s two calls — a start-of-run pid correction and
//! an end-of-run finished marker — as well as on [`super::try_spawn`]'s
//! initial claim. See those callers for why each write happens.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::context::store::ContextStore;

/// File name of the debounce lock, inside the context cache directory.
const LOCK_FILE: &str = "reconcile.lock";

/// Path of the debounce lock, inside the context cache directory.
pub(super) fn reconcile_lock_path(store: &ContextStore) -> PathBuf {
    store.root().join(LOCK_FILE)
}

/// What [`decide`] concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LockDecision {
    /// No live, fresh lock in the way: safe to claim the lock and spawn.
    Spawn,
    /// A fresh lock belongs to a live reconcile, or a finished one is still
    /// within its debounce window: do nothing.
    Skip,
}

/// The debounce decision — see the module doc's policy table. Takes `now`
/// and `is_alive` as parameters rather than reading the clock or the process
/// table itself, so it is a pure function a test can drive deterministically
/// without a real process.
pub(super) fn decide(
    lock_path: &Path,
    now: u64,
    debounce_secs: u64,
    stale_lock_secs: u64,
    is_alive: impl Fn(u32) -> bool,
) -> LockDecision {
    let Some((epoch, pid)) = read_lock(lock_path) else {
        return LockDecision::Spawn;
    };
    let age = now.saturating_sub(epoch);

    if pid == 0 {
        // A finished marker: purely a throttle on how often a NEW attempt
        // may start, independent of any process's liveness.
        return if age < debounce_secs {
            LockDecision::Skip
        } else {
            LockDecision::Spawn
        };
    }

    // An in-progress marker.
    if !is_alive(pid) {
        return LockDecision::Spawn; // crashed; take over regardless of age
    }
    if age >= stale_lock_secs {
        return LockDecision::Spawn; // hung; take over anyway
    }
    LockDecision::Skip
}

/// Parse `"<epoch_secs> <pid>"` from `lock_path`. `None` for a missing,
/// unreadable, or malformed file — all of which [`decide`] treats as "no
/// lock", i.e. eligible for a fresh claim; see the module doc's "Encoding"
/// section for why failing toward SPAWN, not toward silence, is the safe
/// direction for a corrupt file.
///
/// `pub(super)` rather than private: the test suite reads a lock's exact
/// content back to assert what [`claim_lock`] wrote, not just what [`decide`]
/// concluded from it.
pub(super) fn read_lock(lock_path: &Path) -> Option<(u64, u32)> {
    let content = fs::read_to_string(lock_path).ok()?;
    let mut parts = content.split_whitespace();
    let epoch: u64 = parts.next()?.parse().ok()?;
    let pid: u32 = parts.next()?.parse().ok()?;
    Some((epoch, pid))
}

/// Atomically (re)write `lock_path` to `"<now> <pid>"`. Used for all three
/// lock writes this module makes: [`super::try_spawn`]'s initial claim
/// (`pid` = the spawning hook's own pid, `take_over` gates whether an
/// existing lock is being reclaimed), [`super::try_reconcile`]'s pid
/// correction, and its finished marker (`pid == 0`, `take_over = true`).
///
/// `take_over` removes an existing lock file first; either way the final
/// create uses `O_CREAT|O_EXCL` semantics (`create_new`), so two hooks racing
/// to claim the SAME absent lock cannot both win — the loser's `create_new`
/// fails with `AlreadyExists` and this returns `false`, which the ONLY
/// caller that checks it (`try_spawn`) reads as "someone else has this" and
/// skips spawning rather than erroring.
pub(super) fn claim_lock(lock_path: &Path, now: u64, pid: u32, take_over: bool) -> bool {
    if take_over {
        let _ = fs::remove_file(lock_path);
    }
    if let Some(parent) = lock_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let Ok(mut file) = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    else {
        return false;
    };
    file.write_all(format!("{now} {pid}\n").as_bytes()).is_ok()
}

/// Current unix time in whole seconds. `0` on a clock that reads before the
/// epoch (practically unreachable) rather than panicking — this runs on a
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
