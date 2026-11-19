use super::{
    append, open_dir_safely, open_safely, path_to_cstring, validate_relpath, MAX_LOG_BYTES,
};
use crate::fs::{safe_read, safe_replace};
use anyhow::{bail, Context, Result};
use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;

/// Write `content` to `relpath` under `dirfd` with an exclusive flock,
/// refusing every symlink along the path. This in-place writer locks the opened
/// data inode before truncation; it does not provide the crash-atomic
/// replacement semantics of `locking::locked_write`.
pub fn safe_locked_write_in_workdir(dirfd: RawFd, relpath: &Path, content: &[u8]) -> Result<()> {
    let content = enforce_size_limit(content)?;
    let fd = open_locked_regular(dirfd, relpath, libc::O_WRONLY | libc::O_CREAT, 0o600)?;
    // Truncate after locking so cooperating writers do not overwrite each other.
    // SAFETY: `fd` is a live, exclusively locked descriptor opened for writing.
    if unsafe { libc::ftruncate(fd.as_raw_fd(), 0) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: ftruncate failed on {}", relpath.display()));
    }
    write_all_at(&fd, content, relpath)?;
    // fsync to ensure data hits disk before flock releases.
    // SAFETY: `fd` remains live and owned for the duration of the call.
    if unsafe { libc::fsync(fd.as_raw_fd()) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: fsync failed on {}", relpath.display()));
    }
    Ok(())
}

/// Append the complete buffer while holding an exclusive flock; cooperating
/// appenders cannot interleave its partial writes.
pub fn safe_append_in_workdir(dirfd: RawFd, relpath: &Path, content: &[u8]) -> Result<()> {
    let content = enforce_size_limit(content)?;
    write_all_at(&append::open_append_at(dirfd, relpath)?, content, relpath)
}

/// Create `relpath` under `dirfd` with `O_EXCL`: fails if the file already
/// exists. Used for handoffs and other artefacts that must be unique.
pub fn safe_create_new_in_workdir(dirfd: RawFd, relpath: &Path, content: &[u8]) -> Result<()> {
    let content = enforce_size_limit(content)?;
    let fd = open_safely(
        dirfd,
        relpath,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    write_all_at(&fd, content, relpath)?;
    Ok(())
}

/// `mkdirat` for the final component of `relpath` with `O_EXCL` semantics:
/// fails with `EEXIST` if the directory already exists. Intermediate
/// components are walked with `O_NOFOLLOW | O_DIRECTORY` (no auto-create),
/// so an attacker cannot redirect creation through a planted symlink.
pub fn safe_create_new_dir_in_workdir(
    dirfd: RawFd,
    relpath: &Path,
    mode: libc::mode_t,
) -> Result<()> {
    let components = validate_relpath(relpath)?;
    let parent_fd = parent_dirfd_for(dirfd, &components)?;
    let final_name = components.last().unwrap();
    let c = path_to_cstring(final_name)?;
    // SAFETY: parent_fd is valid; c is NUL-terminated.
    let r = unsafe { libc::mkdirat(parent_fd.as_raw_fd(), c.as_ptr(), mode) };
    if r < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: mkdirat failed on {}", relpath.display()));
    }
    Ok(())
}

/// `renameat` under `dirfd`. Both `from` and `to` are validated relpaths;
/// each is anchored at the parent's dirfd resolved via `O_NOFOLLOW`-walked
/// `open_dir_safely`, so neither the source nor the destination component
/// may pass through a symlink.
pub fn safe_rename_in_workdir(dirfd: RawFd, from: &Path, to: &Path) -> Result<()> {
    let from_components = validate_relpath(from)?;
    let to_components = validate_relpath(to)?;

    let from_parent_fd = parent_dirfd_for(dirfd, &from_components)?;
    let to_parent_fd = parent_dirfd_for(dirfd, &to_components)?;

    let from_name = path_to_cstring(from_components.last().unwrap())?;
    let to_name = path_to_cstring(to_components.last().unwrap())?;

    // SAFETY: fds are valid; both names are NUL-terminated.
    let r = unsafe {
        libc::renameat(
            from_parent_fd.as_raw_fd(),
            from_name.as_ptr(),
            to_parent_fd.as_raw_fd(),
            to_name.as_ptr(),
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error()).with_context(|| {
            format!(
                "safe_fs: renameat failed: {} -> {}",
                from.display(),
                to.display()
            )
        });
    }
    Ok(())
}

fn parent_dirfd_for(dirfd: RawFd, components: &[Vec<u8>]) -> Result<OwnedFd> {
    if components.len() == 1 {
        // SAFETY: dup duplicates a valid fd, returning a fresh owned fd.
        let dup = unsafe { libc::dup(dirfd) };
        if dup < 0 {
            return Err(io::Error::last_os_error()).context("safe_fs: dup dirfd failed");
        }
        // SAFETY: successful `dup` returned a fresh descriptor whose ownership
        // is transferred exactly once to `OwnedFd`.
        return Ok(unsafe { OwnedFd::from_raw_fd(dup) });
    }
    let parent_components = &components[..components.len() - 1];
    let parent_path = parent_components
        .iter()
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    open_dir_safely(dirfd, Path::new(&parent_path))
}

/// Atomically replace a file beneath `project_root` through an anchored parent.
/// Intermediate symlinks are refused and the replacement uses private mode.
pub fn safe_replace_outside_workdir(
    target: &Path,
    project_root: &Path,
    content: &[u8],
) -> Result<()> {
    let content = enforce_size_limit(content)?;
    let (root, relative) = safe_replace::split_under_root(target, project_root)?;
    safe_replace::replace_private(&root, relative, content)
}

/// Like `safe_locked_write_in_workdir` but sets the file mode via `fchmod`
/// after open. Used for wrapper scripts that must be executable (0o755).
pub fn safe_write_with_mode_in_workdir(
    dirfd: RawFd,
    relpath: &Path,
    content: &[u8],
    mode: libc::mode_t,
) -> Result<()> {
    let content = enforce_size_limit(content)?;
    #[allow(clippy::useless_conversion)] // mode_t is u32 on Linux but u16 on macOS
    let mode_u32: u32 = mode.into();
    let fd = open_locked_regular(dirfd, relpath, libc::O_WRONLY | libc::O_CREAT, mode_u32)?;
    // SAFETY: `fd` is a live, exclusively locked descriptor opened for writing.
    if unsafe { libc::ftruncate(fd.as_raw_fd(), 0) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: ftruncate failed on {}", relpath.display()));
    }
    write_all_at(&fd, content, relpath)?;
    // Force the requested mode regardless of umask.
    // SAFETY: `fd` is live and `mode` is a platform `mode_t` supplied by the caller.
    if unsafe { libc::fchmod(fd.as_raw_fd(), mode) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: fchmod failed on {}", relpath.display()));
    }
    // SAFETY: `fd` remains live and owned for the duration of the call.
    if unsafe { libc::fsync(fd.as_raw_fd()) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: fsync failed on {}", relpath.display()));
    }
    Ok(())
}

/// Remove `relpath` under `dirfd` via `unlinkat`. Refuses paths with `..`
/// components and absolute prefixes.
pub fn safe_remove_in_workdir(dirfd: RawFd, relpath: &Path) -> Result<()> {
    let components = validate_relpath(relpath)?;
    let parent = components[..components.len() - 1]
        .iter()
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let final_name = components.last().unwrap();
    let parent_fd = if parent.is_empty() {
        // SAFETY: dup duplicates a valid fd, returning a fresh owned fd.
        let dup = unsafe { libc::dup(dirfd) };
        if dup < 0 {
            return Err(io::Error::last_os_error()).context("safe_fs: dup dirfd failed");
        }
        // SAFETY: successful `dup` returned a fresh descriptor whose ownership
        // is transferred exactly once to `OwnedFd`.
        unsafe { OwnedFd::from_raw_fd(dup) }
    } else {
        open_dir_safely(dirfd, Path::new(&parent))?
    };
    let c = path_to_cstring(final_name)?;
    // SAFETY: parent_fd is valid; c is NUL-terminated.
    if unsafe { libc::unlinkat(parent_fd.as_raw_fd(), c.as_ptr(), 0) } < 0 {
        let err = io::Error::last_os_error();
        let raw = err.raw_os_error().unwrap_or(0);
        if raw == libc::ENOENT {
            return Ok(());
        }
        return Err(err)
            .with_context(|| format!("safe_fs: unlinkat failed on {}", relpath.display()));
    }
    Ok(())
}

pub(super) fn open_locked_regular(
    dirfd: RawFd,
    path: &Path,
    flags: i32,
    mode: u32,
) -> Result<OwnedFd> {
    let fd = open_safely(dirfd, path, flags | libc::O_NONBLOCK, mode)?;
    let file = std::fs::File::from(fd);
    safe_read::single_link_regular_file(&file)?;
    let fd = OwnedFd::from(file);
    flock_exclusive(&fd)?;
    let file = std::fs::File::from(fd);
    safe_read::single_link_regular_file(&file)?;
    Ok(OwnedFd::from(file))
}

pub(crate) fn flock_exclusive(fd: &OwnedFd) -> Result<()> {
    // SAFETY: fd is valid; LOCK_EX is a constant.
    if unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX) } < 0 {
        return Err(io::Error::last_os_error()).context("safe_fs: flock LOCK_EX failed");
    }
    Ok(())
}

fn write_all_at(fd: &OwnedFd, mut buf: &[u8], relpath: &Path) -> Result<()> {
    while !buf.is_empty() {
        // SAFETY: fd is valid; buf has the documented length.
        let n = unsafe { libc::write(fd.as_raw_fd(), buf.as_ptr() as *const _, buf.len()) };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err)
                .with_context(|| format!("safe_fs: write failed on {}", relpath.display()));
        }
        if n == 0 {
            bail!("safe_fs: write returned 0 on {}", relpath.display());
        }
        buf = &buf[n as usize..];
    }
    Ok(())
}

fn enforce_size_limit(content: &[u8]) -> Result<&[u8]> {
    if content.len() > MAX_LOG_BYTES {
        bail!(
            "safe_fs: refusing write of {} bytes (limit {} bytes)",
            content.len(),
            MAX_LOG_BYTES
        );
    }
    Ok(content)
}
