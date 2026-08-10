//! Authoritative singleton lock and persisted daemon process identity.

use super::core::DaemonServer;
use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(super) const LOCK_FILE: &str = "orchestrator.lock";
pub(super) const PID_FILE: &str = "orchestrator.pid";
const MAX_IDENTITY_BYTES: usize = 128;

pub(super) enum LockState {
    /// Acquiring the flock proves no daemon owns this state.
    Free(Option<File>),
    /// Another open file description owns the flock.
    Held(Option<crate::process::ProcessIdentity>),
    /// The lock could not be inspected safely; callers must fail closed.
    Indeterminate,
}

pub(super) fn inspect_lock(work_dir: &Path) -> LockState {
    let path = work_dir.join(LOCK_FILE);
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
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

pub(super) fn acquire_lock(work_dir: &Path) -> Result<File> {
    let lock_path = work_dir.join(LOCK_FILE);
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
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

pub(super) fn current_identity() -> crate::process::ProcessIdentity {
    let pid = std::process::id();
    crate::process::ProcessIdentity {
        pid,
        start_time: crate::process::process_start_time(pid),
    }
}

pub(super) fn format_identity(identity: crate::process::ProcessIdentity) -> String {
    match identity.start_time {
        Some(start_time) => format!("{} {start_time}\n", identity.pid),
        None => format!("{} -\n", identity.pid),
    }
}

pub(super) fn read_persisted_identity(work_dir: &Path) -> Option<crate::process::ProcessIdentity> {
    read_identity_file(work_dir, Path::new(PID_FILE))
}

pub(super) fn read_recorded_lock_identity(
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

fn parse_identity(content: &str) -> Option<crate::process::ProcessIdentity> {
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

impl DaemonServer {
    pub(super) fn acquire_exclusive_lock(&self) -> Result<File> {
        acquire_lock(&self.work_dir)
    }

    /// Return the recorded PID only when another process holds the flock.
    pub fn check_lock(work_dir: &Path) -> Option<u32> {
        match inspect_lock(work_dir) {
            LockState::Held(identity) => identity.map(|identity| identity.pid),
            LockState::Free(_) | LockState::Indeterminate => None,
        }
    }

    pub(crate) fn held_identity(
        work_dir: &Path,
    ) -> Result<Option<crate::process::ProcessIdentity>> {
        match inspect_lock(work_dir) {
            LockState::Held(identity) => Ok(identity),
            LockState::Free(_) => Ok(None),
            LockState::Indeterminate => {
                bail!("daemon singleton lock could not be inspected safely")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trip_and_legacy_pid_fail_closed() {
        let identity = crate::process::ProcessIdentity {
            pid: 42,
            start_time: Some(99),
        };
        assert_eq!(parse_identity(&format_identity(identity)), Some(identity));
        assert_eq!(
            parse_identity("42"),
            Some(crate::process::ProcessIdentity {
                pid: 42,
                start_time: None
            })
        );
    }

    /// Poll until the lock reads `Free`, returning the last state seen.
    ///
    /// Releasing is not always observable on the very next probe: any process
    /// this test binary forks concurrently (every other test that spawns a
    /// command) inherits the lock descriptor for the window between fork and
    /// exec, and an inherited copy holds the flock alive even after the owner
    /// closes its own descriptor — `O_CLOEXEC` drops it at exec, not at fork.
    /// The release is still correct; it just is not instantaneous under load.
    fn poll_until_free(work_dir: &Path) -> &'static str {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match inspect_lock(work_dir) {
                LockState::Free(_) => return "free",
                state => {
                    if std::time::Instant::now() >= deadline {
                        return match state {
                            LockState::Held(_) => "held",
                            LockState::Indeterminate => "indeterminate",
                            LockState::Free(_) => "free",
                        };
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }

    #[test]
    fn held_and_free_lock_states_are_distinct() {
        let work_dir = tempfile::tempdir().unwrap();
        let guard = acquire_lock(work_dir.path()).unwrap();
        assert!(matches!(inspect_lock(work_dir.path()), LockState::Held(_)));
        drop(guard);
        assert_eq!(
            poll_until_free(work_dir.path()),
            "free",
            "lock never reported Free within 2s of the guard being dropped"
        );
    }

    #[test]
    fn symlink_lock_is_indeterminate() {
        let work_dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), work_dir.path().join(LOCK_FILE)).unwrap();

        assert!(matches!(
            inspect_lock(work_dir.path()),
            LockState::Indeterminate
        ));
    }
}
