//! The summary store lock.
//!
//! One exclusive `flock` on `<runtime dir>/summaries.lock` serialises every write under
//! `doc/loom/summaries/`: the server's engine, its sessions and a `seal` run from a terminal.
//! A `flock` belongs to the open file description, so the lock is not re-entrant: a caller that
//! already holds it must not call [`with_store_lock`] again (it would wait on itself).

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::path::Path;

use anyhow::{bail, Context, Result};
use fs2::FileExt;

use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::planning::runtime::{create_runtime_dir, RuntimePaths};

/// Holds the exclusive lock; unlocks when dropped, on every exit path.
struct StoreLock {
    file: File,
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        // Closing the descriptor releases the lock as well; the explicit unlock only makes
        // the release immediate.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Create the runtime directory, and the loom user directory above it, when the runtime
/// directory is missing. An existing real directory is used as it is, so a session whose
/// planning root is read-only never tries to create anything.
pub(crate) fn ensure_runtime_dir(paths: &RuntimePaths) -> Result<()> {
    if safe_open_dirfd(&paths.dir).is_ok() {
        return Ok(());
    }
    create_runtime_dir(paths).with_context(|| {
        format!(
            "cannot create the runtime directory {}",
            paths.dir.display()
        )
    })
}

/// True when an open failed because the caller may not write there: EROFS under a read-only bind,
/// EACCES under Landlock or file modes, EPERM under Seatbelt.
fn write_denied(error: &anyhow::Error) -> bool {
    let cause = error.root_cause().downcast_ref::<io::Error>();
    let code = cause.and_then(io::Error::raw_os_error);
    matches!(code, Some(libc::EROFS | libc::EACCES | libc::EPERM))
}

/// Open the lock file `name` under `dirfd` for locking, creating it (mode 0600) when missing.
///
/// A Codex summary job has `drafts/` and `claims/` writable but not this file, so it opens the
/// lock read-only: `flock(2)` needs no write access on a local filesystem and the file holds no
/// content. A file missing then stays an error, since the job could not create it. O_NONBLOCK
/// keeps a planted FIFO from blocking the open (it does not affect `flock`), and the type check
/// refuses it. Revisit if a home on NFS must work there, where an exclusive `flock` needs a
/// writable descriptor.
fn open_lock(dirfd: RawFd, name: &Path) -> Result<OwnedFd> {
    let flags = libc::O_RDWR | libc::O_CREAT;
    match open_safely(dirfd, name, flags, 0o600) {
        Err(error) if write_denied(&error) => {}
        opened => return opened,
    }
    let flags = libc::O_RDONLY | libc::O_NONBLOCK;
    let fd = open_safely(dirfd, name, flags, 0)?;
    if !File::from(fd.try_clone()?).metadata()?.is_file() {
        bail!("{} is not a regular file", name.display());
    }
    Ok(fd)
}

/// Open (creating it, mode 0600) and exclusively lock the store lock file, refusing a
/// symlink at the lock file or any directory component below the runtime directory.
fn acquire(paths: &RuntimePaths) -> Result<StoreLock> {
    ensure_runtime_dir(paths)?;
    let name: &Path = paths
        .summaries_lock
        .strip_prefix(&paths.dir)
        .with_context(|| {
            format!(
                "store lock {} is outside the runtime directory",
                paths.summaries_lock.display()
            )
        })?;
    let dirfd = safe_open_dirfd(&paths.dir)?;
    let fd = open_lock(dirfd.as_raw_fd(), name).with_context(|| {
        format!(
            "cannot open the store lock {}",
            paths.summaries_lock.display()
        )
    })?;
    let file = File::from(fd);
    file.lock_exclusive()
        .with_context(|| format!("cannot lock {}", paths.summaries_lock.display()))?;
    Ok(StoreLock { file })
}

/// Run `f` while holding the exclusive store lock, releasing it however `f` returns.
pub fn with_store_lock<T>(paths: &RuntimePaths, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let guard = acquire(paths)?;
    let result = f();
    drop(guard);
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;
    use crate::planning::runtime::runtime_paths_under;

    fn runtime() -> (TempDir, RuntimePaths) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("project");
        fs::create_dir_all(&root).unwrap();
        let paths = runtime_paths_under(&tmp.path().join("home"), &root).unwrap();
        create_runtime_dir(&paths).unwrap();
        (tmp, paths)
    }

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn each_sandbox_write_denial_counts_and_a_missing_file_does_not() {
        let denied = |code| write_denied(&io::Error::from_raw_os_error(code).into());
        for code in [libc::EROFS, libc::EACCES, libc::EPERM] {
            assert!(denied(code), "errno {code}");
        }
        assert!(!denied(libc::ENOENT));
    }

    #[test]
    fn a_read_only_lock_file_still_serialises() {
        let (_tmp, paths) = runtime();
        fs::write(&paths.summaries_lock, b"").unwrap();
        set_mode(&paths.summaries_lock, 0o400);
        let held = with_store_lock(&paths, || {
            let other = File::open(&paths.summaries_lock)?;
            Ok(other.try_lock_exclusive().is_err())
        });
        assert!(held.unwrap(), "the lock is held through the fallback");
        let other = File::open(&paths.summaries_lock).unwrap();
        other.try_lock_exclusive().expect("released on return");
    }

    #[test]
    fn a_lock_file_missing_from_a_read_only_directory_stays_an_error() {
        let (_tmp, paths) = runtime();
        set_mode(&paths.dir, 0o500);
        let result = with_store_lock(&paths, || Ok(()));
        set_mode(&paths.dir, 0o700);
        let error = format!("{:#}", result.expect_err("no file to lock"));
        assert!(error.contains("cannot open the store lock"), "{error}");
        assert!(!paths.summaries_lock.exists());
    }
}
