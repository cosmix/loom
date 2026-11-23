//! Tests for lease paths and for reading lock files in the session-writable
//! cache directory.

use super::super::{try_claim, Claim};
use super::*;
use tempfile::TempDir;

const LOCK_FILE: &str = "reconcile.lock";
const LINE: &str = "100 0 0 0\n";

#[test]
fn lease_paths_differ_per_checkout() {
    let cache = TempDir::new().unwrap();
    let store = ContextStore::with_root(cache.path().join("cache"));
    let checkout_a = TempDir::new().unwrap();
    let checkout_b = TempDir::new().unwrap();

    let lock_a = reconcile_lock_path(&store, checkout_a.path());
    let lock_b = reconcile_lock_path(&store, checkout_b.path());

    assert_ne!(lock_a, lock_b);
    assert_eq!(lock_a, reconcile_lock_path(&store, checkout_a.path()));
    for lock_path in [lock_a, lock_b] {
        assert_eq!(lock_path.parent(), Some(store.root()));
        let name = lock_path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("reconcile-"));
        assert!(name.ends_with(".lock"));
    }
}

#[test]
fn a_live_holder_in_one_checkout_does_not_queue_a_request_from_another() {
    let cache = TempDir::new().unwrap();
    let store = ContextStore::with_root(cache.path().join("cache"));
    store.ensure().unwrap();
    let checkout_a = TempDir::new().unwrap();
    let checkout_b = TempDir::new().unwrap();
    let lock_a = reconcile_lock_path(&store, checkout_a.path());
    let lock_b = reconcile_lock_path(&store, checkout_b.path());

    assert_eq!(
        try_claim(&lock_a, 1_000, 600, 1_800, |_| true, 4_242),
        Claim::Won
    );
    assert_eq!(
        try_claim(&lock_b, 1_001, 600, 1_800, |_| true, 4_343),
        Claim::Won
    );
    assert!(!read_lock(&lock_a).unwrap().pending);
}

fn lock_with(content: &str) -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join(LOCK_FILE);
    std::fs::write(&lock_path, content).unwrap();
    (temp, lock_path)
}

#[test]
fn a_plain_lock_file_reads_back() {
    let (_temp, lock_path) = lock_with(LINE);

    assert_eq!(
        read_lock(&lock_path),
        Some(LockState {
            epoch: 100,
            pid: 0,
            failures: 0,
            pending: false,
            claim: 0,
        })
    );
}

#[test]
fn a_lock_file_past_the_size_bound_reads_as_no_lock() {
    let (_temp, lock_path) = lock_with(&format!("{LINE}{}", " ".repeat(MAX_LOCK_BYTES)));

    assert_eq!(read_lock(&lock_path), None);
}

#[cfg(unix)]
#[test]
fn a_symlinked_lock_reads_as_no_lock() {
    let (temp, real) = lock_with(LINE);
    let link = temp.path().join("linked.lock");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    assert!(read_lock(&real).is_some(), "control: the target parses");
    assert_eq!(read_lock(&link), None);
}

#[cfg(unix)]
#[test]
fn a_fifo_in_place_of_the_lock_reads_as_no_lock_without_blocking() {
    let temp = TempDir::new().unwrap();
    let lock_path = temp.path().join(LOCK_FILE);
    nix::unistd::mkfifo(&lock_path, nix::sys::stat::Mode::S_IRWXU).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    // A blocked read leaves this thread behind and fails the test on the
    // timeout instead of hanging the run.
    std::thread::spawn(move || {
        let _ = sender.send(read_lock(&lock_path));
    });

    let read = receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("read_lock blocked on a FIFO");

    assert_eq!(read, None);
}
