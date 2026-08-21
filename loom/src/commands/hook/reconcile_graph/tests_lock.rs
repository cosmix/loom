//! Tests for [`super::decide`] and [`super::claim_lock`] — pure, process-free.
//!
//! `decide` and `claim_lock` take the clock and the liveness check as
//! parameters, so a test drives them without a real, killable process at a
//! known pid and without spawning or waiting on anything. Split out of
//! `tests_reconcile_graph.rs` (which keeps the `reconcile_graph()`/
//! `spawn_if_needed()` end-to-end tests) so neither file grows past the
//! maintainability line limit.

use super::*;
use tempfile::TempDir;

const DEBOUNCE_SECS: u64 = 600;
const STALE_LOCK_SECS: u64 = 1800;

// ---------------------------------------------------------------------------
// The debounce decision.
// ---------------------------------------------------------------------------

#[test]
fn decide_spawns_when_no_lock_exists() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");

    let decision = decide(
        &lock_path,
        1_000_000,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );

    assert_eq!(decision, LockDecision::Spawn);
}

#[test]
fn decide_skips_a_young_in_progress_lock_with_a_live_pid() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    assert!(claim_lock(&lock_path, now, 4242, false));

    // 10 minutes later, well under the stale-lock ceiling.
    let decision = decide(
        &lock_path,
        now + 600,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );

    assert_eq!(decision, LockDecision::Skip);
}

#[test]
fn decide_takes_over_an_in_progress_lock_with_a_dead_pid_regardless_of_age() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    assert!(claim_lock(&lock_path, now, 4242, false));

    // Just 5 seconds old — "young" by any reading — but the owner is dead.
    let decision = decide(&lock_path, now + 5, DEBOUNCE_SECS, STALE_LOCK_SECS, |_| {
        false
    });

    assert_eq!(
        decision,
        LockDecision::Spawn,
        "a dead-owned lock must be taken over even when it is very young"
    );
}

#[test]
fn decide_takes_over_an_in_progress_lock_older_than_the_stale_ceiling_even_if_alive() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    assert!(claim_lock(&lock_path, now, 4242, false));

    let decision = decide(
        &lock_path,
        now + STALE_LOCK_SECS,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );

    assert_eq!(
        decision,
        LockDecision::Spawn,
        "the stale-lock ceiling is a paranoia backstop independent of liveness"
    );
}

#[test]
fn decide_spawns_over_a_corrupt_lock_file() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    std::fs::write(&lock_path, "not a valid lock body").unwrap();

    let decision = decide(
        &lock_path,
        1_000_000,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );

    assert_eq!(
        decision,
        LockDecision::Spawn,
        "a corrupt lock must never wedge self-healing shut forever"
    );
}

#[test]
fn decide_skips_a_finished_marker_younger_than_the_debounce_window() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    assert!(claim_lock(&lock_path, now, 0, false), "a finished marker");

    let decision = decide(&lock_path, now + 60, DEBOUNCE_SECS, STALE_LOCK_SECS, |_| {
        true
    });

    assert_eq!(
        decision,
        LockDecision::Skip,
        "a finished marker inside the debounce window must throttle a new attempt"
    );
}

#[test]
fn decide_spawns_over_a_finished_marker_older_than_the_debounce_window() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    assert!(claim_lock(&lock_path, now, 0, false));

    let decision = decide(
        &lock_path,
        now + DEBOUNCE_SECS,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );

    assert_eq!(
        decision,
        LockDecision::Spawn,
        "the debounce window elapsed; a new attempt is due"
    );
}

#[test]
fn decide_spawns_over_a_finished_marker_with_a_garbage_timestamp() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    std::fs::write(&lock_path, "not-a-timestamp 0\n").unwrap();

    let decision = decide(
        &lock_path,
        1_000_000,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );

    assert_eq!(
        decision,
        LockDecision::Spawn,
        "an unparseable finished marker must not wedge self-healing shut, same as any other corrupt lock"
    );
}

// ---------------------------------------------------------------------------
// Lock claim mechanics.
// ---------------------------------------------------------------------------

#[test]
fn claim_lock_writes_the_epoch_and_pid() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");

    assert!(claim_lock(&lock_path, 12345, 999, false));

    assert_eq!(read_lock(&lock_path), Some((12345, 999)));
}

#[test]
fn claim_lock_without_take_over_fails_against_an_existing_lock() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    assert!(claim_lock(&lock_path, 1, 1, false));

    let second = claim_lock(&lock_path, 2, 2, false);

    assert!(
        !second,
        "O_CREAT|O_EXCL must refuse a second claim over a live lock"
    );
    assert_eq!(
        read_lock(&lock_path),
        Some((1, 1)),
        "a lost race must not overwrite the winner's lock"
    );
}

#[test]
fn claim_lock_with_take_over_replaces_an_existing_lock() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    assert!(claim_lock(&lock_path, 1, 1, false));

    let second = claim_lock(&lock_path, 2, 2, true);

    assert!(second, "a take-over claim must replace a stale lock");
    assert_eq!(read_lock(&lock_path), Some((2, 2)));
}
