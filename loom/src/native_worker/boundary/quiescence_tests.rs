//! The namespace kill signals only members of the namespace it names.
use std::process::Command;
use std::time::Duration;

use super::{kill_namespace, Quiescence};
use crate::test_stand_in::{require_signals, StandIn};

/// A host process named with another namespace is never signalled: a pid the
/// boundary's first process left behind and the host reused stays alive.
#[test]
fn killing_a_namespace_spares_a_host_process_outside_it() {
    require_signals!();
    let mut sleep = StandIn::direct(Command::new("sleep").arg("30"));
    let pid = i32::try_from(sleep.id()).unwrap();
    // No pid namespace has inode 1, so it has no members either.
    let quiescence = kill_namespace(pid, 1);
    std::thread::sleep(Duration::from_millis(100));
    let alive = sleep.child().try_wait().unwrap().is_none();
    drop(sleep);
    assert_eq!(quiescence, Quiescence::Released);
    assert!(alive, "a process outside the namespace was killed");
}
