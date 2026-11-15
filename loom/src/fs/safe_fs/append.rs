//! The one opener for append-only logs: no symlink at any component beneath
//! the anchor directory, a regular single-link file, private mode on create,
//! and an exclusive `flock` held for the life of the returned handle.

use anyhow::Result;
use std::fs::File;
use std::os::unix::io::{AsRawFd, OwnedFd, RawFd};
use std::path::Path;

use super::{open_locked_regular, safe_open_dirfd};

/// Open `relpath` beneath `dir` for appending, creating it with mode 0600.
///
/// `dir` is opened `O_NOFOLLOW`, so `dir` itself must not be a symlink
/// (symlinks above its final component are followed); `relpath` must be
/// relative without `..`, and no component of it may be a symlink. The file
/// must be a regular file with one link. The exclusive lock is released when
/// the returned `File` drops; size caps, headers and fsync are caller policy.
pub(crate) fn safe_open_append(dir: &Path, relpath: &Path) -> Result<File> {
    let dirfd = safe_open_dirfd(dir)?;
    open_append_at(dirfd.as_raw_fd(), relpath).map(File::from)
}

/// [`safe_open_append`] anchored at an already-open directory.
pub(super) fn open_append_at(dirfd: RawFd, relpath: &Path) -> Result<OwnedFd> {
    open_locked_regular(
        dirfd,
        relpath,
        libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT,
        0o600,
    )
}
