//! macOS teardown of a boundary whose input the caller closed.

use std::time::Duration;

use crate::native_worker::boundary::{kill_mac, private_root, Quiescence};
use crate::native_worker::model::{MacIdentity, EOF_GRACE_SECS};
use crate::process::OwnedProcess;

/// A grace of `EOF_GRACE_SECS` for the leader to exit, without reaping it;
/// then `kill_mac`, while the unreaped leader still pins the session and group
/// number; then the leader is retired and reaped. The private invocation root
/// goes only after `Released`; on `Unverified` it stays with the record.
pub(super) fn retire_mac(process: &mut OwnedProcess, mac: &MacIdentity) -> Quiescence {
    // Never reaped here: reaping would free its pid, and with it the group
    // number `kill_mac` signals. A wait error leaves the grace to `kill_mac`.
    let _ = process.wait_unreaped(Duration::from_secs(EOF_GRACE_SECS));
    let quiescence = kill_mac(mac);
    process.retire();
    private_root::reclaim(Some(mac), quiescence).unwrap_or_else(Quiescence::Unverified)
}
