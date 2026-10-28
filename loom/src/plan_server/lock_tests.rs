//! Tests for the server lock.

use super::lock::{acquire, Acquired};
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn published_record_is_visible_to_second_opener() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock");
    let Acquired::Held(mut lock) = acquire(&path).unwrap() else {
        panic!("lock not acquired")
    };
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    lock.publish("http://127.0.0.1:7272", "secret").unwrap();
    let Acquired::Running(record) = acquire(&path).unwrap() else {
        panic!("lock not held")
    };
    assert_eq!(record.pid, std::process::id());
    assert_eq!(record.url.as_deref(), Some("http://127.0.0.1:7272"));
    assert_eq!(record.token.as_deref(), Some("secret"));
    drop(lock);
    assert!(matches!(acquire(&path).unwrap(), Acquired::Held(_)));
}

#[test]
fn a_dropped_lock_is_free_while_a_child_still_holds_its_descriptor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock");
    let Acquired::Held(lock) = acquire(&path).unwrap() else {
        panic!("lock not acquired")
    };
    // The copy a child spawned by another thread keeps until its exec.
    let inherited = lock.duplicate().unwrap();
    drop(lock);
    assert!(matches!(acquire(&path).unwrap(), Acquired::Held(_)));
    drop(inherited);
}

#[test]
fn symlink_is_refused_without_touching_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    let path = dir.path().join("lock");
    std::fs::write(&target, "keep").unwrap();
    symlink(&target, &path).unwrap();
    let error = match acquire(&path) {
        Err(error) => error,
        Ok(_) => panic!("symlink accepted"),
    };
    assert!(error.to_string().contains(&path.display().to_string()));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
}

#[test]
fn incomplete_or_oversized_record_does_not_prevent_reporting_running() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock");
    let Acquired::Held(lock) = acquire(&path).unwrap() else {
        panic!("lock not acquired")
    };
    for contents in ["{".to_owned(), "x".repeat(4097)] {
        std::fs::write(&path, contents).unwrap();
        let Acquired::Running(record) = acquire(&path).unwrap() else {
            panic!("lock not held")
        };
        assert!(record.url.is_none());
        assert!(record.token.is_none());
    }
    drop(lock);
}

#[test]
fn non_regular_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    assert!(acquire(dir.path()).is_err());
}
