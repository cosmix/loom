use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use super::lease::{
    acquire, finish, prune_results, Acquired, BootClock, LeaseDir, RESULT_RETENTION_SECS,
};
use super::model::{TerminalOutcome, WaitLease};
use super::tests::{acquire_owner, fixture, identity, owner, Fixture};
use crate::process::IdentityStatus;

fn active(dir: &LeaseDir) -> WaitLease {
    let bytes = fs::read(dir.root().join("lease.json")).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn result(dir: &LeaseDir, wait_id: &str) -> WaitLease {
    let bytes = fs::read(dir.root().join("results").join(format!("{wait_id}.json"))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn acquire_simultaneously(fixture: &Fixture) -> Vec<Acquired> {
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let barrier = barrier.clone();
        let dir = fixture.dir.clone();
        let identity = fixture.identity.clone();
        let clock = fixture.clock.clone();
        let probe = fixture.probe.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            acquire(
                &dir,
                &identity,
                Duration::from_secs(30),
                "revision-a",
                &clock,
                &probe,
            )
            .unwrap()
        }));
    }
    handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect()
}

fn assert_mode(path: &Path, expected: u32) {
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        expected
    );
}

fn assert_private_lease_directories(fixture: &Fixture) {
    let mut path = fixture._temp.path().to_path_buf();
    for component in fixture.dir.root().strip_prefix(&path).unwrap().components() {
        path.push(component);
        assert_mode(&path, 0o700);
    }
    assert_mode(&fixture.dir.root().join("results"), 0o700);
}

#[test]
fn simultaneous_matching_watchers_share_one_unchanged_lease() {
    let fixture = fixture();
    let acquired = acquire_simultaneously(&fixture);
    let owner_lease = acquired.iter().find_map(|item| match item {
        Acquired::Owner(lease) => Some(lease),
        _ => None,
    });
    let duplicate = acquired.iter().find_map(|item| match item {
        Acquired::AlreadyWaiting(lease) => Some(lease),
        _ => None,
    });

    assert_eq!(
        acquired
            .iter()
            .filter(|item| matches!(item, Acquired::Owner(_)))
            .count(),
        1
    );
    assert_eq!(
        acquired
            .iter()
            .filter(|item| matches!(item, Acquired::AlreadyWaiting(_)))
            .count(),
        1
    );
    assert_eq!(owner_lease, duplicate);
    assert_eq!(Some(&active(&fixture.dir)), owner_lease);
}

#[test]
fn different_worker_set_reports_busy_with_existing_wait_id() {
    let fixture = fixture();
    let existing = acquire_owner(&fixture, Duration::from_secs(30));
    let mut other = fixture.identity.clone();
    other.workers.pop();

    let acquired = acquire(
        &fixture.dir,
        &other,
        Duration::from_secs(30),
        "revision-b",
        &fixture.clock,
        &fixture.probe,
    )
    .unwrap();

    match acquired {
        Acquired::Busy(lease) => assert_eq!(lease.wait_id, existing.wait_id),
        other => panic!("expected busy, got {other:?}"),
    }
}

#[test]
fn recycled_pid_interrupts_old_wait_and_creates_owner() {
    let fixture = fixture();
    let old = acquire_owner(&fixture, Duration::from_secs(30));
    fixture.probe.set_status(old.owner, IdentityStatus::Dead);
    fixture.probe.set_current(owner(old.owner.pid, Some(11)));

    let acquired = acquire(
        &fixture.dir,
        &fixture.identity,
        Duration::from_secs(30),
        "revision-b",
        &fixture.clock,
        &fixture.probe,
    )
    .unwrap();

    let Acquired::Owner(new) = acquired else {
        panic!("expected replacement owner")
    };
    assert_ne!(new.wait_id, old.wait_id);
    assert_eq!(new.owner, owner(old.owner.pid, Some(11)));
    assert_eq!(
        result(&fixture.dir, &old.wait_id).terminal_result,
        Some(TerminalOutcome::Interrupted)
    );
}

#[test]
fn unverifiable_owner_is_kept_until_deadline_then_times_out() {
    let fixture = fixture();
    let old = acquire_owner(&fixture, Duration::from_secs(10));
    fixture
        .probe
        .set_status(old.owner, IdentityStatus::Unverifiable);

    let before = acquire(
        &fixture.dir,
        &fixture.identity,
        Duration::from_secs(10),
        "revision-b",
        &fixture.clock,
        &fixture.probe,
    )
    .unwrap();
    assert!(matches!(before, Acquired::AlreadyWaiting(ref lease) if lease.wait_id == old.wait_id));

    fixture.clock.set_monotonic_ns(10 * 1_000_000_000);
    let after = acquire(
        &fixture.dir,
        &fixture.identity,
        Duration::from_secs(10),
        "revision-c",
        &fixture.clock,
        &fixture.probe,
    )
    .unwrap();
    assert!(matches!(after, Acquired::Owner(ref lease) if lease.wait_id != old.wait_id));
    assert_eq!(
        result(&fixture.dir, &old.wait_id).terminal_result,
        Some(TerminalOutcome::TimedOut)
    );
}

#[test]
fn changed_boot_interrupts_old_wait() {
    let fixture = fixture();
    let old = acquire_owner(&fixture, Duration::from_secs(30));
    fixture.clock.set_boot_id("boot-b");

    let acquired = acquire(
        &fixture.dir,
        &fixture.identity,
        Duration::from_secs(30),
        "revision-b",
        &fixture.clock,
        &fixture.probe,
    )
    .unwrap();

    assert!(matches!(acquired, Acquired::Owner(ref lease) if lease.wait_id != old.wait_id));
    assert_eq!(
        result(&fixture.dir, &old.wait_id).terminal_result,
        Some(TerminalOutcome::Interrupted)
    );
}

#[test]
fn finish_removes_only_the_exact_active_lease() {
    let fixture = fixture();
    let lease = acquire_owner(&fixture, Duration::from_secs(30));
    let mut wrong_owner = lease.clone();
    wrong_owner.owner = owner(999, Some(99));
    finish(
        &fixture.dir,
        &wrong_owner,
        TerminalOutcome::Failed,
        &fixture.clock,
    )
    .unwrap();
    assert_eq!(active(&fixture.dir), lease);

    let mut wrong_id = lease.clone();
    wrong_id.wait_id = "different-wait".into();
    finish(
        &fixture.dir,
        &wrong_id,
        TerminalOutcome::Cancelled,
        &fixture.clock,
    )
    .unwrap();
    assert_eq!(active(&fixture.dir), lease);

    finish(
        &fixture.dir,
        &lease,
        TerminalOutcome::Succeeded,
        &fixture.clock,
    )
    .unwrap();
    assert!(!fixture.dir.root().join("lease.json").exists());
    assert_eq!(
        result(&fixture.dir, &lease.wait_id).terminal_result,
        Some(TerminalOutcome::Succeeded)
    );
}

fn write_result_fixture(dir: &LeaseDir, lease: &WaitLease) {
    let path = dir
        .root()
        .join("results")
        .join(format!("{}.json", lease.wait_id));
    fs::write(path, serde_json::to_vec_pretty(lease).unwrap()).unwrap();
}

#[test]
fn pruning_removes_only_expired_terminal_results() {
    let fixture = fixture();
    let template = acquire_owner(&fixture, Duration::from_secs(30));
    fixture.clock.set_unix_secs(RESULT_RETENTION_SECS + 1);
    let now = fixture.clock.unix_secs();
    let mut old = template.clone();
    old.wait_id = "old".into();
    old.terminal_result = Some(TerminalOutcome::Succeeded);
    old.finished_unix_secs = Some(now - RESULT_RETENTION_SECS - 1);
    write_result_fixture(&fixture.dir, &old);
    let mut fresh = old.clone();
    fresh.wait_id = "fresh".into();
    fresh.finished_unix_secs = Some(now - RESULT_RETENTION_SECS);
    write_result_fixture(&fixture.dir, &fresh);
    let mut pending = old.clone();
    pending.wait_id = "pending".into();
    pending.terminal_result = None;
    write_result_fixture(&fixture.dir, &pending);
    fs::write(
        fixture.dir.root().join("results/malformed.json"),
        b"not json",
    )
    .unwrap();

    assert_eq!(prune_results(&fixture.dir, &fixture.clock).unwrap(), 1);
    assert!(!fixture.dir.root().join("results/old.json").exists());
    assert!(fixture.dir.root().join("results/fresh.json").exists());
    assert!(fixture.dir.root().join("results/pending.json").exists());
    assert!(fixture.dir.root().join("results/malformed.json").exists());
}

#[test]
fn malformed_and_symlinked_active_leases_are_not_overwritten() {
    let malformed = fixture();
    let path = malformed.dir.root().join("lease.json");
    fs::write(&path, b"broken lease").unwrap();
    let error = acquire(
        &malformed.dir,
        &malformed.identity,
        Duration::from_secs(30),
        "revision-a",
        &malformed.clock,
        &malformed.probe,
    );
    assert!(error.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken lease");

    let linked = fixture();
    let target = linked._temp.path().join("target");
    fs::write(&target, b"sentinel").unwrap();
    let path = linked.dir.root().join("lease.json");
    symlink(&target, &path).unwrap();
    let error = acquire(
        &linked.dir,
        &linked.identity,
        Duration::from_secs(30),
        "revision-a",
        &linked.clock,
        &linked.probe,
    );
    assert!(error.is_err());
    assert!(fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(target).unwrap(), b"sentinel");
}

#[test]
fn symlinked_directory_component_is_refused() {
    let temp = tempfile::Builder::new()
        .prefix("loom-wait-symlink-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let identity = identity();
    let prepared = LeaseDir::open(temp.path(), &identity).unwrap();
    let component = prepared.root().to_path_buf();
    drop(prepared);
    fs::remove_dir(component.join("results")).unwrap();
    fs::remove_dir(&component).unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    symlink(&target, &component).unwrap();

    let error = LeaseDir::open(temp.path(), &identity).unwrap_err();

    assert!(error
        .to_string()
        .contains("refusing symlink directory component"));
    assert!(!target.join("lease.json").exists());
}

#[test]
fn created_directories_and_files_have_private_modes() {
    let fixture = fixture();
    let lease = acquire_owner(&fixture, Duration::from_secs(30));
    assert_private_lease_directories(&fixture);
    assert_mode(&fixture.dir.root().join("lease.json"), 0o600);

    finish(
        &fixture.dir,
        &lease,
        TerminalOutcome::Succeeded,
        &fixture.clock,
    )
    .unwrap();
    let result = fixture
        .dir
        .root()
        .join("results")
        .join(format!("{}.json", lease.wait_id));
    assert_mode(&result, 0o600);
}
