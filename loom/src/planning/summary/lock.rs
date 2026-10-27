//! The summary store lock.
//!
//! One exclusive `flock` on `<runtime dir>/summaries.lock` serialises every write under
//! `doc/loom/summaries/`: the server's engine, its sessions and a `seal` run from a terminal.
//! A `flock` belongs to the open file description, so the lock is not re-entrant: a caller that
//! already holds it must not call [`with_store_lock`] again (it would wait on itself).

use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{Context, Result};
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
    let flags = libc::O_RDWR | libc::O_CREAT;
    let fd = open_safely(dirfd.as_raw_fd(), name, flags, 0o600).with_context(|| {
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
