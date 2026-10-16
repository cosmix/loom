//! Authoritative singleton lock and persisted daemon process identity.

use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) const LOCK_FILE: &str = "orchestrator.lock";
pub(crate) const PID_FILE: &str = "orchestrator.pid";
const MAX_IDENTITY_BYTES: usize = 128;

pub(crate) enum LockState {
    /// Acquiring the flock proves no daemon owns this state.
    Free(Option<File>),
    /// Another open file description owns the flock.
    Held(Option<crate::process::ProcessIdentity>),
    /// The lock could not be inspected safely; callers must fail closed.
    Indeterminate,
}

pub(crate) fn inspect_lock(work_dir: &Path) -> LockState {
    let path = work_dir.join(LOCK_FILE);
    // O_NONBLOCK keeps a FIFO planted at the lock path from blocking this
    // open forever; it has no effect on a regular file. `proven_stopped`
    // (`observer.rs`) still refuses to trust a `Free` result unless the
    // opened file's metadata says it is a regular file.
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return LockState::Free(None),
        Err(_) => return LockState::Indeterminate,
    };

    // SAFETY: `file` owns a live descriptor for the lock inode, and the flock
    // flags are valid for a non-blocking exclusive ownership probe.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        LockState::Free(Some(file))
    } else {
        let errno = std::io::Error::last_os_error();
        if errno.kind() == std::io::ErrorKind::WouldBlock {
            LockState::Held(read_identity_from_file(&file))
        } else {
            LockState::Indeterminate
        }
    }
}

pub(crate) fn acquire_lock(work_dir: &Path) -> Result<File> {
    let lock_path = work_dir.join(LOCK_FILE);
    // O_NONBLOCK: same reasoning as `inspect_lock` above, for a FIFO planted
    // at the lock path before this daemon starts. It has no effect on the
    // regular file this creates and holds open for the rest of its life.
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&lock_path)
        .context("failed to open daemon singleton lock without following symlinks")?;
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;

    // SAFETY: `file` owns a live descriptor that remains in the returned guard,
    // and the flags request a valid non-blocking exclusive flock.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        let identity = read_identity_from_file(&file);
        if let Some(identity) = identity {
            bail!(
                "another daemon instance holds the singleton lock (recorded PID {})",
                identity.pid
            );
        }
        bail!("another daemon instance holds the singleton lock");
    }

    let identity = current_identity();
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(format_identity(identity).as_bytes())?;
    file.flush()?;
    Ok(file)
}

pub(crate) fn current_identity() -> crate::process::ProcessIdentity {
    let pid = std::process::id();
    crate::process::ProcessIdentity {
        pid,
        start_time: crate::process::process_start_time(pid),
    }
}

pub(crate) fn format_identity(identity: crate::process::ProcessIdentity) -> String {
    match identity.start_time {
        Some(start_time) => format!("{} {start_time}\n", identity.pid),
        None => format!("{} -\n", identity.pid),
    }
}

pub(crate) fn read_persisted_identity(work_dir: &Path) -> Option<crate::process::ProcessIdentity> {
    read_identity_file(work_dir, Path::new(PID_FILE))
}

pub(crate) fn read_recorded_lock_identity(
    work_dir: &Path,
) -> Option<crate::process::ProcessIdentity> {
    read_identity_file(work_dir, Path::new(LOCK_FILE))
}

fn read_identity_file(work_dir: &Path, relative: &Path) -> Option<crate::process::ProcessIdentity> {
    let content =
        crate::fs::safe_read::read_to_string_bounded(work_dir, relative, MAX_IDENTITY_BYTES)
            .ok()?;
    parse_identity(&content)
}

fn read_identity_from_file(file: &File) -> Option<crate::process::ProcessIdentity> {
    let mut file = file;
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut content = String::new();
    file.take(MAX_IDENTITY_BYTES as u64)
        .read_to_string(&mut content)
        .ok()?;
    parse_identity(&content)
}

pub(crate) fn parse_identity(content: &str) -> Option<crate::process::ProcessIdentity> {
    let mut fields = content.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let start_time = match fields.next() {
        Some("-") | None => None,
        Some(value) => Some(value.parse().ok()?),
    };
    if fields.next().is_some() {
        return None;
    }
    Some(crate::process::ProcessIdentity { pid, start_time })
}

/// Lock ownership is authoritative; an unreadable lock fails closed.
pub(crate) fn is_running(work_dir: &Path) -> bool {
    !matches!(inspect_lock(work_dir), LockState::Free(_))
}
