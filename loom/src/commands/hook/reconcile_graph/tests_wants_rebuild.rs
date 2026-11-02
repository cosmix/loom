//! Rebuild, backoff, holder concurrency, and cancellation tests.
//!
//! Children are reaped; `SPAWN_ENABLED` stays false under `cfg(test)`.

use super::lock::{decide, read_lock, update_lock, LockDecision, LockState};
use super::tests::{degraded_pack, leave};
use super::*;
use serial_test::serial;
use std::os::unix::process::ExitStatusExt;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

const DEBOUNCE_SECS: u64 = 600;
const STALE_LOCK_SECS: u64 = 1800;

pub(super) fn seed(lock_path: &Path, epoch: u64, pid: u32, failures: u32, pending: bool) {
    let state = LockState {
        epoch,
        pid,
        failures,
        pending,
        claim: 0,
    };
    update_lock(lock_path, |_| (Some(state), ())).expect("seed the lock");
}

/// Kill and reap a child when its test ends.
struct Reaped(std::process::Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn fake_reconciler() -> Reaped {
    let child = Command::new("sh")
        .args(["-c", "read line", "hook", "reconcile-graph"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let execed = |pid| cancel::process_argv(pid).is_some_and(|a| a.contains(&"hook".into()));
    while !execed(child.id()) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Reaped(child)
}

fn lock_in(temp: &TempDir) -> PathBuf {
    temp.path().join("reconcile.lock")
}

/// A project root with a state directory, pinned through `LOOM_WORK_DIR`.
fn pinned_project(temp: &TempDir) -> ContextStore {
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::env::remove_var("LOOM_STAGE_ID");
    std::env::set_var("LOOM_WORK_DIR", &work);
    let store = ContextStore::open(&WorkDir::new(temp.path()).unwrap()).unwrap();
    store.ensure().unwrap();
    store
}

fn built(stale: bool) -> Freshness {
    Freshness {
        revision: "abc".into(),
        stale,
        ..Freshness::default()
    }
}

#[test]
fn wants_rebuild_follows_the_state_table() {
    for degraded in [None, Some("degraded")] {
        assert!(wants_rebuild(&built(true), degraded));
        assert!(!wants_rebuild(&Freshness::never_built("x"), degraded));
        assert!(!wants_rebuild(&Freshness::unavailable("x"), degraded));
    }
    assert!(wants_rebuild(&built(false), Some("degraded")));
    assert!(!wants_rebuild(&built(false), None));
}

#[test]
#[serial]
fn never_built_degraded_pack_spawns_nothing() {
    let temp = TempDir::new().unwrap();
    let store = pinned_project(&temp);
    let before = SUPPRESSED_SPAWNS.load(Ordering::SeqCst);
    let never_built = ContextPack {
        semantic_freshness: Freshness::never_built("x"),
        ..degraded_pack()
    };

    spawn_if_needed(&never_built, temp.path());

    assert_eq!(SUPPRESSED_SPAWNS.load(Ordering::SeqCst), before);
    assert!(!reconcile_lock_path(&store).exists());

    // Control: the same environment does spawn for a built, degraded graph.
    spawn_if_needed(&degraded_pack(), temp.path());
    leave();
    assert_eq!(SUPPRESSED_SPAWNS.load(Ordering::SeqCst), before + 1);
}

#[test]
#[serial]
fn two_failed_runs_count_two_failures() {
    let temp = TempDir::new().unwrap();
    let store = pinned_project(&temp);
    let lock_path = reconcile_lock_path(&store);
    let debounce = RetrievalConfig::load(temp.path()).reconcile_debounce_secs;
    let stale = RetrievalConfig::load(temp.path()).reconcile_stale_lock_secs;
    let config = RetrievalConfig::load(temp.path());

    try_spawn(temp.path(), unix_now()).unwrap();
    let claim = read_lock(&lock_path).unwrap().claim;
    run_passes(&lock_path, Some(claim), &config, || true);
    let first = read_lock(&lock_path).unwrap();
    assert_eq!((first.pid, first.failures), (0, 1), "{first:?}");
    let refused = decide(
        &lock_path,
        first.epoch + debounce * 2 - 1,
        debounce,
        stale,
        |_| true,
    );
    assert_eq!(refused, LockDecision::Skip);

    try_spawn(temp.path(), first.epoch + debounce * 2).unwrap();
    assert_eq!(read_lock(&lock_path).unwrap().pid, std::process::id());
    let claim = read_lock(&lock_path).unwrap().claim;
    run_passes(&lock_path, Some(claim), &config, || true);
    leave();

    let second = read_lock(&lock_path).unwrap();
    assert_eq!((second.pid, second.failures), (0, 2), "{second:?}");
    let window = debounce * 4;
    let before = decide(
        &lock_path,
        second.epoch + window - 1,
        debounce,
        stale,
        |_| true,
    );
    let after = decide(&lock_path, second.epoch + window, debounce, stale, |_| true);
    assert_eq!(before, LockDecision::Skip);
    assert_eq!(after, LockDecision::Spawn);
}

#[test]
fn pending_pass_keeps_the_lease_held() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let me = std::process::id();
    let mut passes = 0;
    let config = RetrievalConfig::load(temp.path());

    run_passes(&lock_path, None, &config, || {
        passes += 1;
        if passes > 1 {
            let held = read_lock(&lock_path).unwrap();
            assert_eq!((held.pid, held.pending), (me, false), "pass {passes}");
        }
        if passes < 3 {
            // A request arrives while this pass runs: it queues, never spawns.
            let claim = try_claim(
                &lock_path,
                unix_now(),
                DEBOUNCE_SECS,
                STALE_LOCK_SECS,
                |_| true,
                7,
            );
            assert_eq!(claim, Claim::Queued, "pass {passes}");
        }
        false
    });

    assert_eq!(passes, 3, "each queued request earns exactly one more pass");
    let done = read_lock(&lock_path).unwrap();
    assert_eq!((done.pid, done.failures, done.pending), (0, 0, false));
}

#[test]
fn finish_after_takeover_keeps_the_new_holder() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let config = RetrievalConfig::load(temp.path());

    run_passes(&lock_path, None, &config, || {
        seed(&lock_path, 200, 4242, 3, true);
        true
    });

    assert_eq!(
        std::fs::read_to_string(&lock_path).unwrap(),
        "200 4242 3 1\n"
    );
}

#[test]
fn pending_is_never_lost_across_threads() {
    const HOLDER: u32 = 4242;
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let now = unix_now();

    for round in 0..100 {
        seed(&lock_path, now, HOLDER, 0, false);
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let reader = scope.spawn(|| {
                while !stop.load(Ordering::SeqCst) {
                    assert!(
                        read_lock(&lock_path).is_some(),
                        "round {round}: empty or partial lock"
                    );
                }
            });
            let claimer = scope.spawn(|| {
                try_claim(
                    &lock_path,
                    now + 1,
                    DEBOUNCE_SECS,
                    STALE_LOCK_SECS,
                    |_| true,
                    7,
                )
            });
            let finisher = scope.spawn(|| end_pass(&lock_path, now + 2, HOLDER, 0, false));
            let (claim, end) = (claimer.join().unwrap(), finisher.join().unwrap());
            stop.store(true, Ordering::SeqCst);
            reader.join().unwrap();

            match (claim, end) {
                // The request landed first: the holder saw it and keeps the lease.
                (Claim::Queued, PassEnd::Again) => {
                    let held = read_lock(&lock_path).unwrap();
                    assert_eq!((held.pid, held.pending), (HOLDER, false), "round {round}");
                }
                // The holder finished first: the request met a throttled marker.
                (Claim::Refused, PassEnd::Done) => {
                    assert_eq!(read_lock(&lock_path).unwrap().pid, 0, "round {round}");
                }
                other => panic!("round {round}: a queued request was lost: {other:?}"),
            }
        });
    }
}

#[test]
fn cancel_skips_a_decoy_process() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let mut decoy = Reaped(Command::new("sleep").arg("30").spawn().unwrap());
    seed(&lock_path, unix_now(), decoy.0.id(), 2, true);

    cancel::cancel_holder(&lock_path, unix_now());

    assert!(
        decoy.0.try_wait().unwrap().is_none(),
        "the decoy must survive"
    );
    let released = read_lock(&lock_path).unwrap();
    assert_eq!(
        (released.pid, released.failures, released.pending),
        (0, 2, false)
    );
}

#[test]
fn cancel_signals_a_holder_that_is_the_reconciler() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let mut holder = fake_reconciler();
    seed(&lock_path, unix_now(), holder.0.id(), 1, false);

    cancel::cancel_holder(&lock_path, unix_now());

    assert_eq!(holder.0.wait().unwrap().signal(), Some(libc::SIGTERM));
    let released = read_lock(&lock_path).unwrap();
    assert_eq!((released.pid, released.failures), (0, 1));
}

#[test]
fn cancel_leaves_a_process_that_started_after_the_lease() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let mut newcomer = fake_reconciler();
    // A lease stamped an hour ago cannot belong to a process that just started.
    seed(&lock_path, unix_now() - 3600, newcomer.0.id(), 0, false);

    cancel::cancel_holder(&lock_path, unix_now());

    assert!(
        newcomer.0.try_wait().unwrap().is_none(),
        "a reused pid must survive"
    );
    assert_eq!(read_lock(&lock_path).unwrap().pid, 0);
}

#[test]
fn cancel_with_no_holder_writes_nothing() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);

    cancel::cancel_holder(&lock_path, unix_now());
    assert!(!lock_path.exists());

    seed(&lock_path, 100, 0, 2, false);
    cancel::cancel_holder(&lock_path, unix_now());
    assert_eq!(std::fs::read_to_string(&lock_path).unwrap(), "100 0 2 0\n");
}

#[test]
fn cancel_with_a_dead_holder_leaves_the_lease_for_an_immediate_takeover() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let now = unix_now();
    seed(&lock_path, now, u32::MAX, 1, false);

    cancel::cancel_holder(&lock_path, now);

    assert_eq!(read_lock(&lock_path).unwrap().pid, u32::MAX);
    let decision = decide(&lock_path, now, 600, 1800, crate::process::is_process_alive);
    assert_eq!(decision, LockDecision::Spawn);
}

#[test]
fn failed_spawn_releases_the_lease_with_a_failure_and_backoff() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let me = std::process::id();
    let now = 5_000;
    seed(&lock_path, now, me, 1, true);

    let result = spawn_claimed(&lock_path, now, me, |_| {
        Err(anyhow::anyhow!("spawn refused"))
    });

    assert!(result.is_err(), "the spawn error is still returned");
    let released = read_lock(&lock_path).unwrap();
    let expected = LockState {
        epoch: now,
        pid: 0,
        failures: 2,
        pending: true,
        claim: 0,
    };
    assert_eq!(released, expected);
    let window = DEBOUNCE_SECS * 4;
    let throttled = decide(
        &lock_path,
        now + window - 1,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );
    let elapsed = decide(
        &lock_path,
        now + window,
        DEBOUNCE_SECS,
        STALE_LOCK_SECS,
        |_| true,
    );
    assert_eq!(throttled, LockDecision::Skip);
    assert_eq!(elapsed, LockDecision::Spawn);
}

#[test]
fn failed_spawn_leaves_a_taken_over_lease_alone() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    seed(&lock_path, 200, 4242, 3, false);

    let result = spawn_claimed(&lock_path, 300, std::process::id(), |_| {
        Err(anyhow::anyhow!("spawn refused"))
    });

    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(&lock_path).unwrap(),
        "200 4242 3 0\n"
    );
}

#[test]
fn successful_spawn_keeps_the_lease_claimed() {
    let temp = TempDir::new().unwrap();
    let lock_path = lock_in(&temp);
    let me = std::process::id();
    seed(&lock_path, 300, me, 0, false);

    spawn_claimed(&lock_path, 300, me, |_| Ok(None)).unwrap();

    assert_eq!(read_lock(&lock_path).unwrap().pid, me);
}
