//! Tests for [`super::rebind_claim`]: the spawning hook hands its lease to the
//! child it spawned. Pure over a tempdir lock file; the "child" is this
//! process, the only pid a test can rely on being alive.

use super::*;
use tempfile::TempDir;

const DEBOUNCE_SECS: u64 = 600;
const STALE_LOCK_SECS: u64 = 1800;
/// Above `i32::MAX`, so `is_process_alive` reports it dead without a process.
const DEAD_PID: u32 = u32::MAX;

fn seed(lock_path: &Path, epoch: u64, pid: u32) {
    let state = LockState {
        epoch,
        pid,
        failures: 0,
        pending: false,
        claim: 0,
    };
    update_lock(lock_path, |_| (Some(state), ())).expect("seed the lock");
}

fn second_hook(lock_path: &Path, now: u64) -> LockDecision {
    decide(
        lock_path,
        now,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        crate::process::is_process_alive,
    )
}

#[test]
fn a_rebound_claim_names_the_child_and_blocks_a_second_hook() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    let child = std::process::id();
    seed(&lock_path, now, DEAD_PID);
    assert_eq!(second_hook(&lock_path, now), LockDecision::Spawn);

    rebind_claim(&lock_path, now, DEAD_PID, child, 0);

    assert_eq!(read_lock(&lock_path).unwrap().pid, child);
    assert_eq!(second_hook(&lock_path, now), LockDecision::Skip);
}

#[test]
fn a_rebind_does_not_touch_a_lease_that_moved_on() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let now = 1_000_000;
    let child = std::process::id();

    seed(&lock_path, now + 5, 4242);
    rebind_claim(&lock_path, now, 4242, child, 0);
    assert_eq!(read_lock(&lock_path).unwrap().pid, 4242, "other epoch");

    seed(&lock_path, now, 4243);
    rebind_claim(&lock_path, now, 4242, child, 0);
    assert_eq!(read_lock(&lock_path).unwrap().pid, 4243, "other holder");
}

#[test]
fn child_startup_requires_its_exact_claim() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let parent = DEAD_PID;
    let child = std::process::id();
    update_lock(&lock_path, |_| {
        (
            Some(LockState {
                claim: 11,
                ..held_by(None, 100, parent)
            }),
            (),
        )
    })
    .unwrap();

    assert!(!begin_run(&lock_path, child, 10, 101));
    assert_eq!(read_lock(&lock_path).unwrap().pid, parent);
    assert!(begin_run(&lock_path, child, 11, 101));
    assert_eq!(read_lock(&lock_path).unwrap().pid, child);

    update_lock(&lock_path, |_| {
        (
            Some(LockState {
                claim: 12,
                ..held_by(None, 100, parent)
            }),
            (),
        )
    })
    .unwrap();
    assert!(
        !begin_run(&lock_path, child, 11, 101),
        "delayed child lost its claim"
    );
    assert_eq!(read_lock(&lock_path).unwrap().claim, 12);
}

/// `--cancel` trusts a holder only if it started by the lock's epoch. A child
/// spawned late against its parent's claim would fail that check unless its
/// startup re-stamps the epoch.
#[test]
fn child_startup_restamps_the_epoch_and_keeps_the_counters() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let state = LockState {
        epoch: 100,
        pid: DEAD_PID,
        failures: 3,
        pending: true,
        claim: 11,
    };
    update_lock(&lock_path, |_| (Some(state), ())).unwrap();

    assert!(begin_run(&lock_path, 4242, 11, 500));

    assert_eq!(
        read_lock(&lock_path),
        Some(LockState {
            epoch: 500,
            pid: 4242,
            ..state
        })
    );
}

#[test]
fn denied_startups_execute_no_pass() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join("reconcile.lock");
    let config = crate::context::config::RetrievalConfig::load(temp.path());
    let mut passes = 0;
    seed(&lock_path, unix_now(), std::process::id());
    super::super::run_passes(&lock_path, None, &config, || {
        passes += 1;
        false
    });
    assert_eq!(passes, 0, "a live holder denies a direct invocation");

    let blocker = temp.path().join("blocker");
    std::fs::write(&blocker, b"file, not a directory").unwrap();
    let unwritable = blocker.join("reconcile.lock");
    super::super::run_passes(&unwritable, None, &config, || {
        passes += 1;
        false
    });
    assert_eq!(passes, 0, "a failed lease write denies startup");
}
