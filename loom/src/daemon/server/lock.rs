//! Daemon adapter for neutral singleton state storage.
use super::core::DaemonServer;
pub(super) use crate::fs::daemon_state::*;
use anyhow::{bail, Result};
use std::fs::File;
use std::path::Path;
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

    /// A FIFO with no writer would block a plain `open()` forever; `inspect_lock`
    /// must return promptly instead (on another thread, so a regression fails
    /// the test rather than hanging it), and whatever it reports must not read
    /// as proof the lock is free and regular: `proven_stopped`
    /// (`observer.rs`) only trusts `Free(Some(file))` when that file's
    /// metadata says it is a regular file, which a FIFO's never does.
    #[test]
    fn fifo_lock_does_not_block_inspect() {
        let work_dir = tempfile::tempdir().unwrap();
        let path = work_dir.path().join(LOCK_FILE);
        let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: `mkfifo` with a NUL-terminated path and a valid mode creates
        // a FIFO special file; it does not open or block on it.
        let result = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
        assert_eq!(
            result,
            0,
            "mkfifo failed: {}",
            std::io::Error::last_os_error()
        );

        let dir = work_dir.path().to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let state = inspect_lock(&dir);
            let is_regular_when_free = match &state {
                LockState::Free(Some(file)) => file.metadata().is_ok_and(|meta| meta.is_file()),
                LockState::Free(None) => false,
                LockState::Held(_) | LockState::Indeterminate => false,
            };
            sender.send(is_regular_when_free).unwrap();
        });

        let is_regular_when_free = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("inspect_lock blocked on a FIFO");
        assert!(
            !is_regular_when_free,
            "a FIFO at the lock path must never read as a regular, free lock"
        );
    }
}
