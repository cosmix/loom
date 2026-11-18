//! The boundary marker of a scratch bound into a native-worker pid namespace.
//!
//! A supervisor that cannot prove its boundary's namespace empty keeps the
//! scratch on disk: the writable Cargo home inside it is still bound into
//! processes that may be running. The marker `.loom-boundary`, at the scratch
//! root (never bound into the boundary), holds `<boot id> [<pid namespace>]`.
//! The sweep reclaims such a scratch only once the host rebooted or no process
//! is left in that namespace; with the namespace unknown (bubblewrap failed
//! before reporting it), only after a reboot. The marker's boot id decides
//! whenever it and this boot's id can be read; a marker that cannot be read
//! or parsed keeps the scratch only while it last changed in this boot.
use std::io::{ErrorKind, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use anyhow::{Context, Result};

/// The marker's file name at the scratch root.
pub(super) const NAME: &str = ".loom-boundary";
/// Bound on the marker text read: a boot id and a namespace inode.
const MAX_BYTES: u64 = 256;

/// Record that `scratch` is bound into a boundary of this boot, and its pid
/// namespace once known. Never follows a symlink at the marker's path.
pub(in super::super) fn write(scratch: &Path, pid_namespace: Option<u64>) -> Result<()> {
    use std::io::Write;
    let boot = crate::process::boot_id::os_boot_id()?;
    let text = match pid_namespace {
        Some(namespace) => format!("{boot} {namespace}\n"),
        None => format!("{boot}\n"),
    };
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(scratch.join(NAME))
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .context("record the boundary of a command scratch")
}

/// Whether a boundary `scratch` is bound into may still have live members.
/// No marker means no boundary. A marker naming a boot, with this boot's id
/// readable, decides: another boot has no members left, this one leaves it to
/// the namespace's membership (unknown or unreadable: it may). Only a marker
/// that cannot be read or parsed, or an unreadable boot id, falls back to the
/// scratch's age: one from before this boot has no members left.
pub(super) fn may_live(scratch: &Path) -> bool {
    may_live_since(scratch, boot_time())
}

/// [`may_live`] with the boot at `boot` seconds since the epoch, unknown when
/// `None`.
pub(super) fn may_live_since(scratch: &Path, boot: Option<i64>) -> bool {
    let text = match read(scratch) {
        Ok(None) => return false,
        Ok(Some(text)) => text,
        Err(()) => String::new(),
    };
    let mut fields = text.split_whitespace();
    let current = crate::process::boot_id::os_boot_id().ok();
    let (Some(marked), Some(current)) = (fields.next(), current) else {
        return !predates(scratch, boot);
    };
    if marked != current {
        return false;
    }
    match fields.next().map(str::parse::<u64>) {
        Some(Ok(namespace)) => !crate::verify::criteria::pid_namespace::members(namespace)
            .is_ok_and(|members| members.is_empty()),
        _ => true,
    }
}

/// Whether `scratch` last changed before `boot`. Its ctime is never earlier
/// than its creation, so a ctime before the boot proves it older. Boot time
/// is the wall clock less the uptime: a wall clock stepped forward after boot
/// (a paused VM, then an NTP step) by more than the uptime at the scratch's
/// last change dates that scratch before the boot. Only a marker that cannot
/// be read or parsed, or an unreadable boot id, relies on this, so the step
/// cannot reclaim a scratch whose marker names this boot. Revisit if hosts
/// that run workers start with a wrong clock.
fn predates(scratch: &Path, boot: Option<i64>) -> bool {
    let Some(boot) = boot else {
        return false;
    };
    std::fs::symlink_metadata(scratch).is_ok_and(|metadata| metadata.ctime() < boot)
}

/// The `btime` line of `/proc/stat`: this boot, in seconds since the epoch.
fn boot_time() -> Option<i64> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    stat.lines()
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()
}

/// The marker text, `None` without one. Opened without following a symlink
/// and without blocking (a planted FIFO), and read only as a regular file.
fn read(scratch: &Path) -> Result<Option<String>, ()> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(scratch.join(NAME));
    let file = match file {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(());
    }
    let mut text = String::new();
    file.take(MAX_BYTES)
        .read_to_string(&mut text)
        .map_err(drop)?;
    Ok(Some(text))
}
