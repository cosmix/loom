//! Cancelling a lease holder that does not exist.

use super::*;

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
