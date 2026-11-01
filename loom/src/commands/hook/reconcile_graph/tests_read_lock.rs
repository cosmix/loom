//! Tests for [`super::read_lock`] against what a sandboxed session can put in
//! the lock's place: the cache directory is writable by the session.

use super::*;
use tempfile::TempDir;

const LINE: &str = "100 0 0 0\n";

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
