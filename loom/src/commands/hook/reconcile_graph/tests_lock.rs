//! Tests for [`super::decide`], [`super::super::try_claim`] and the lock line —
//! pure, process-free.
//!
//! `decide` and `try_claim` take the clock and the liveness check as
//! parameters, so a test drives them without a real, killable process at a
//! known pid and without spawning or waiting on anything. Split out of
//! `tests_reconcile_graph.rs` (which keeps the `reconcile_graph()`/
//! `spawn_if_needed()` end-to-end tests) so neither file grows past the
//! maintainability line limit.

use super::super::{try_claim, Claim};
use super::*;
use tempfile::TempDir;

const DEBOUNCE_SECS: u64 = 600;
const STALE_LOCK_SECS: u64 = 1800;

fn seed(lock_path: &Path, epoch: u64, pid: u32, failures: u32, pending: bool) {
    let state = LockState {
        epoch,
        pid,
        failures,
        pending,
        claim: 0,
    };
    update_lock(lock_path, |_| (Some(state), ())).expect("seed the lock");
}

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
    seed(&lock_path, now, 4242, 0, false);

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
    seed(&lock_path, now, 4242, 0, false);

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
    seed(&lock_path, now, 4242, 0, false);

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
    seed(&lock_path, now, 0, 0, false);

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
    seed(&lock_path, now, 0, 0, false);

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

#[test]
fn decide_throttles_a_finished_marker_for_the_failure_backed_off_window() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    seed(&lock_path, now, 0, 2, false);
    let window = DEBOUNCE_SECS * 4;

    let inside = decide(
        &lock_path,
        now + window - 1,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );
    let after = decide(
        &lock_path,
        now + window,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );

    assert_eq!(inside, LockDecision::Skip);
    assert_eq!(after, LockDecision::Spawn);
}

#[test]
fn backoff_doubles_per_failure_and_caps() {
    let windows: Vec<u64> = (0..=7)
        .map(|failures| backoff_secs(100, failures))
        .collect();
    assert_eq!(windows, [100, 200, 400, 800, 1600, 3200, 3200, 3200]);
    assert_eq!(backoff_secs(1000, 5), 21_600, "capped at six hours");
    assert_eq!(
        backoff_secs(30_000, 0),
        30_000,
        "the cap never undercuts the base debounce"
    );
    assert_eq!(backoff_secs(u64::MAX, 5), u64::MAX, "no overflow");
}

// ---------------------------------------------------------------------------
// The lock line.
// ---------------------------------------------------------------------------

#[test]
fn the_lock_line_is_four_fields() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");

    seed(&lock_path, 12345, 999, 3, true);

    assert_eq!(
        std::fs::read_to_string(&lock_path).unwrap(),
        "12345 999 3 1\n"
    );
    assert_eq!(
        read_lock(&lock_path),
        Some(LockState {
            epoch: 12345,
            pid: 999,
            failures: 3,
            pending: true,
            claim: 0,
        })
    );
}

#[test]
fn a_line_that_is_not_four_or_five_fields_is_no_lock() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");

    for body in [
        "12345 999\n",
        "12345 999 3\n",
        "12345 999 3 1 7 8\n",
        "12345 999 3 2\n",
        "12345 999 x 0\n",
        "",
    ] {
        std::fs::write(&lock_path, body).unwrap();
        assert_eq!(read_lock(&lock_path), None, "{body:?}");
        assert_eq!(
            decide(&lock_path, 1, DEBOUNCE_SECS, STALE_LOCK_SECS, |_| true),
            LockDecision::Spawn,
            "{body:?} must not wedge healing"
        );
    }
}

// ---------------------------------------------------------------------------
// Claiming and queueing.
// ---------------------------------------------------------------------------

fn claim(lock_path: &Path, now: u64, pid: u32) -> Claim {
    try_claim(
        lock_path,
        now,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
        pid,
    )
}

#[test]
fn try_claim_over_no_lock_wins_with_zeroed_counters() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("nested/reconcile.lock");

    assert_eq!(claim(&lock_path, 50, 7), Claim::Won);

    let state = read_lock(&lock_path).unwrap();
    assert_eq!(
        (state.epoch, state.pid, state.failures, state.pending),
        (50, 7, 0, false)
    );
    assert_ne!(state.claim, 0, "a new lease needs a claim token");
    assert_eq!(
        std::fs::read_to_string(&lock_path).unwrap(),
        format!("50 7 0 0 {}\n", state.claim)
    );
}

#[test]
fn try_claim_over_an_expired_finished_marker_carries_failures_forward() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    seed(&lock_path, 1, 0, 3, false);

    assert_eq!(
        claim(&lock_path, 1 + backoff_secs(DEBOUNCE_SECS, 3), 7),
        Claim::Won
    );

    let state = read_lock(&lock_path).unwrap();
    assert_eq!((state.pid, state.failures, state.pending), (7, 3, false));
}

#[test]
fn try_claim_is_refused_inside_the_backoff_window_and_writes_nothing() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    seed(&lock_path, 100, 0, 1, false);
    let before = std::fs::read_to_string(&lock_path).unwrap();

    assert_eq!(claim(&lock_path, 101, 7), Claim::Refused);

    assert_eq!(std::fs::read_to_string(&lock_path).unwrap(), before);
}

#[test]
fn a_request_behind_a_live_holder_queues_pending_once_and_keeps_its_lease() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    seed(&lock_path, 100, 4242, 2, false);

    assert_eq!(claim(&lock_path, 110, 7), Claim::Queued);
    assert_eq!(claim(&lock_path, 120, 8), Claim::Queued);

    let state = read_lock(&lock_path).unwrap();
    assert_eq!(
        (state.epoch, state.pid, state.failures, state.pending),
        (100, 4242, 2, true),
        "the holder's epoch and pid stay; only pending is set"
    );
}

#[test]
fn pending_is_consumed_by_exactly_one_extra_pass() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    seed(&lock_path, 100, 4242, 0, true);

    assert_eq!(end_pass(&lock_path, 200, 4242, 0, false), PassEnd::Again);
    let kept = read_lock(&lock_path).unwrap();
    assert_eq!((kept.epoch, kept.pid, kept.pending), (200, 4242, false));

    assert_eq!(end_pass(&lock_path, 300, 4242, 0, false), PassEnd::Done);
    let released = read_lock(&lock_path).unwrap();
    assert_eq!(
        (released.epoch, released.pid, released.failures),
        (300, 0, 0)
    );
}

#[test]
fn a_failed_pass_adds_one_failure_and_a_success_resets_them() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    seed(&lock_path, 100, 4242, 2, false);

    assert_eq!(end_pass(&lock_path, 200, 4242, 0, true), PassEnd::Done);
    assert_eq!(read_lock(&lock_path).unwrap().failures, 3);

    seed(&lock_path, 300, 4242, 3, false);
    assert_eq!(end_pass(&lock_path, 400, 4242, 0, false), PassEnd::Done);
    assert_eq!(read_lock(&lock_path).unwrap().failures, 0);
}
