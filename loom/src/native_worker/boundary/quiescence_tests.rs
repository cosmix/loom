//! The namespace kill signals only members of the namespace it names.
use std::process::Command;
use std::time::Duration;

use super::{kill_namespace, Quiescence};
use crate::process::sandbox_probe::reap::kill_and_reap;
use crate::process::sandbox_probe::skip_without_group_signals;

/// A host process named with another namespace is never signalled: a pid the
/// boundary's first process left behind and the host reused stays alive.
#[test]
fn killing_a_namespace_spares_a_host_process_outside_it() {
    let false = skip_without_group_signals("killing_a_namespace_spares_a_host_process_outside_it")
    else {
        return;
    };
    let mut sleep = Command::new("sleep").arg("30").spawn().unwrap();
    let pid = i32::try_from(sleep.id()).unwrap();
    // No pid namespace has inode 1, so it has no members either.
    let quiescence = kill_namespace(pid, 1);
    std::thread::sleep(Duration::from_millis(100));
    let alive = sleep.try_wait().unwrap().is_none();
    kill_and_reap(&mut sleep);
    assert_eq!(quiescence, Quiescence::Released);
    assert!(alive, "a process outside the namespace was killed");
}
