//! Safe filesystem helpers for `.loom/work/` writes that refuse to follow symlinks.
//!
//! All operations are anchored at a directory file descriptor (dirfd) opened
//! with `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, then traverse path components
//! with explicit `O_NOFOLLOW` at every level so that an attacker who can plant
//! a symlink anywhere along the path cannot redirect a `.loom/work/` write to an
//! arbitrary file (`MN8`).
//!
//! The preferred path is one kernel-enforced open of the whole relative path
//! (`single_open`): `openat2(2)` with `RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH |
//! RESOLVE_NO_MAGICLINKS` on Linux, `openat(2)` with `O_NOFOLLOW_ANY` on macOS.
//! When neither is available (older kernels, seccomp, Apple Container) the
//! portable component-by-component openat walk takes over, refusing
//! intermediate or terminal symlinks via `O_NOFOLLOW`.
//!
//! Universally refused: paths containing `..` components, absolute paths,
//! symlinks at any segment of the resolved path.

use anyhow::{bail, Context, Result};
use std::ffi::{CString, OsStr};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Component, Path};

mod append;
mod single_open;
mod write;
pub(crate) use append::safe_open_append;
#[cfg(test)]
pub(crate) use single_open::force_disable_openat2_for_tests;
pub(crate) use write::flock_exclusive;
use write::open_locked_regular;
pub use write::{
    safe_append_in_workdir, safe_create_new_dir_in_workdir, safe_create_new_in_workdir,
    safe_locked_write_in_workdir, safe_remove_in_workdir, safe_rename_in_workdir,
    safe_replace_outside_workdir, safe_write_with_mode_in_workdir,
};

/// Maximum bytes accepted in one write or append call. Append callers must
/// enforce any total-file or retention bound separately.
pub const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;

/// Open `work_dir` as a dirfd suitable as a base for the rest of this module.
///
/// The dirfd has `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`; if `work_dir` itself
/// is a symlink the call fails.
pub fn safe_open_dirfd(work_dir: &Path) -> Result<OwnedFd> {
    let c = path_to_cstring(work_dir.as_os_str().as_bytes())?;
    let flags = libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_RDONLY;
    // SAFETY: c is a NUL-terminated string with the documented flags.
    let fd = unsafe { libc::open(c.as_ptr(), flags) };
    if fd < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("Failed to open dirfd at {}", work_dir.display()));
    }
    // SAFETY: open returned a valid fd we now own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Validate that `relpath` is a relative path with no `..` components and no
/// absolute prefix. Returns the cleaned set of components on success.
fn validate_relpath(relpath: &Path) -> Result<Vec<Vec<u8>>> {
    if relpath.is_absolute() {
        bail!(
            "safe_fs: refusing absolute path {} — must be relative to work_dir",
            relpath.display()
        );
    }
    let mut out = Vec::new();
    for comp in relpath.components() {
        match comp {
            Component::Normal(s) => out.push(s.as_bytes().to_vec()),
            Component::CurDir => {}
            Component::ParentDir => {
                bail!(
                    "safe_fs: refusing path {} — '..' components are not permitted",
                    relpath.display()
                );
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "safe_fs: refusing path {} — must be relative to work_dir",
                    relpath.display()
                );
            }
        }
    }
    if out.is_empty() {
        bail!(
            "safe_fs: refusing empty path {} — must name a file",
            relpath.display()
        );
    }
    Ok(out)
}

fn path_to_cstring(bytes: &[u8]) -> Result<CString> {
    CString::new(bytes).context("safe_fs: path contains interior NUL byte")
}

/// Bounded retries for a racing `O_CREAT` open. Small: the window is a single
/// lookup, so a loser converges in one or two attempts.
const CREATE_RACE_RETRIES: u32 = 8;

/// `openat`, retrying a spurious `ENOENT` from a concurrent create.
///
/// On macOS an `O_CREAT` open can fail with `ENOENT` when several threads
/// create the same name at once: a loser's path lookup observes the entry
/// mid-creation and reports it missing, even though the file exists moments
/// later. Verified with 16 concurrent appenders to one file - 1-9 of them
/// failed per run, while the same run against a pre-created file never failed.
///
/// Only `ENOENT` on an `O_CREAT` open is retried. A symlink rejection
/// (`ELOOP` on Linux, `ENOTDIR` on macOS, both from `O_NOFOLLOW`) is never
/// retried, so this module's refusal to traverse symlinks is unchanged - as is
/// `EEXIST` from `O_EXCL`, which is a meaningful answer rather than a race.
fn openat_retrying_create_race(dirfd: RawFd, path: &CString, flags: i32, mode: u32) -> i32 {
    // SAFETY: dirfd is a valid dirfd; path is NUL-terminated.
    let mut fd = unsafe { libc::openat(dirfd, path.as_ptr(), flags, mode) };
    if flags & libc::O_CREAT == 0 {
        return fd;
    }
    let mut attempts = 0;
    while fd < 0
        && attempts < CREATE_RACE_RETRIES
        && io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT)
    {
        attempts += 1;
        // SAFETY: same invariants as the first call.
        fd = unsafe { libc::openat(dirfd, path.as_ptr(), flags, mode) };
    }
    fd
}

/// Portable `openat`-walk: open each intermediate directory component with
/// `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, then open the final component with
/// the caller's flags. Refuses every symlink along the way (kernel returns
/// `ELOOP` from `O_NOFOLLOW` on a symlink, which we surface as a clear error).
fn portable_open_walk(
    dirfd: RawFd,
    components: &[Vec<u8>],
    final_flags: i32,
    final_mode: u32,
) -> Result<RawFd> {
    let mut cur: RawFd = dirfd;
    let mut owned: Option<OwnedFd> = None;
    for (i, comp) in components.iter().enumerate() {
        let last = i == components.len() - 1;
        let c = path_to_cstring(comp)?;
        let (flags, mode) = if last {
            (final_flags | libc::O_NOFOLLOW | libc::O_CLOEXEC, final_mode)
        } else {
            (
                libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_RDONLY,
                0,
            )
        };
        let fd = openat_retrying_create_race(cur, &c, flags, mode);
        if fd < 0 {
            let err = io::Error::last_os_error();
            // A newline in a name would split this message, and refusal problems are
            // split on newlines. Escaped with std because `fs` must not depend on
            // `verify`, which owns the richer `escaped`; the mkdirat messages do the same.
            return Err(err)
                .with_context(|| format!("safe_fs: openat failed on '{}'", comp.escape_ascii()));
        }
        // Drop any previous intermediate fd we owned.
        if last {
            // Don't wrap the final fd as OwnedFd here — caller takes ownership.
            // But we still need to close the previous intermediate fd.
            drop(owned);
            return Ok(fd);
        }
        // SAFETY: fd is valid; we now own it.
        let next = unsafe { OwnedFd::from_raw_fd(fd) };
        cur = next.as_raw_fd();
        owned = Some(next);
    }
    unreachable!("validate_relpath rejects empty paths")
}

/// Open a file under `dirfd` with the requested flags/mode, refusing every
/// symlink along the path. Tries the single-syscall open first (`openat2` on
/// Linux, `O_NOFOLLOW_ANY` on macOS); falls back to the portable openat-walk
/// if unsupported.
pub(crate) fn open_safely(dirfd: RawFd, relpath: &Path, flags: i32, mode: u32) -> Result<OwnedFd> {
    let components = validate_relpath(relpath)?;
    let fast_flags = flags | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    if let Some(fd) = single_open::try_single_open(dirfd, &joined(&components), fast_flags, mode)? {
        // SAFETY: the single-syscall open returned a valid fd we now own.
        return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
    }

    let fd = portable_open_walk(dirfd, &components, flags, mode)?;
    // SAFETY: portable walk returned a valid fd we now own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Validated components joined by `/`.
fn joined(components: &[Vec<u8>]) -> Vec<u8> {
    components.join(&b'/')
}

/// Open a directory (intermediate) under `dirfd`, refusing every symlink.
fn open_dir_safely(dirfd: RawFd, relpath: &Path) -> Result<OwnedFd> {
    let flags = libc::O_DIRECTORY | libc::O_RDONLY;
    open_safely(dirfd, relpath, flags, 0)
}

/// Create every component of `relpath` as a directory under `dirfd`,
/// refusing to traverse any symlink. Idempotent: existing directories are
/// accepted, existing non-directories are rejected.
///
/// The walk starts at the deepest prefix that already exists, opened through
/// `open_safely`, so an existing prefix's own ancestors are never opened one
/// by one: under a Seatbelt profile that hides them, creating inside a
/// readable grant still works.
pub fn safe_create_dir_all_in_workdir(
    dirfd: RawFd,
    relpath: &Path,
    mode: libc::mode_t,
) -> Result<()> {
    let components = validate_relpath(relpath)?;
    let (depth, mut owned) = deepest_existing_dir(dirfd, &components)?;
    let mut cur: RawFd = owned.as_ref().map_or(dirfd, AsRawFd::as_raw_fd);
    for comp in &components[depth..] {
        let c = path_to_cstring(comp)?;
        // SAFETY: cur is a valid dirfd; c is NUL-terminated.
        let r = unsafe { libc::mkdirat(cur, c.as_ptr(), mode) };
        if r < 0 {
            let err = io::Error::last_os_error();
            let raw = err.raw_os_error().unwrap_or(0);
            if raw != libc::EEXIST {
                return Err(err).with_context(|| {
                    format!("safe_fs: mkdirat failed on '{}'", comp.escape_ascii())
                });
            }
        }
        // Open the (now-existing) directory with O_NOFOLLOW to confirm it's
        // a real directory and not a symlink that was planted between mkdir
        // and the next iteration.
        let open_flags = libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_RDONLY;
        // SAFETY: cur is a valid dirfd; c is NUL-terminated.
        let fd = unsafe { libc::openat(cur, c.as_ptr(), open_flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error()).with_context(|| {
                format!(
                    "safe_fs: openat after mkdir failed on '{}'",
                    comp.escape_ascii()
                )
            });
        }
        // SAFETY: fd is valid; we now own it.
        let next = unsafe { OwnedFd::from_raw_fd(fd) };
        cur = next.as_raw_fd();
        owned = Some(next);
    }
    drop(owned);
    Ok(())
}

/// The depth of the longest prefix of `components` that opens as a directory,
/// with its descriptor; `(0, None)` when even the first component is missing.
/// Only `ENOENT` moves to a shorter prefix: a symlink, a non-directory or a
/// denied open is an error, as the walk would report it.
fn deepest_existing_dir(dirfd: RawFd, components: &[Vec<u8>]) -> Result<(usize, Option<OwnedFd>)> {
    for depth in (1..=components.len()).rev() {
        let prefix = joined(&components[..depth]);
        match open_dir_safely(dirfd, Path::new(OsStr::from_bytes(&prefix))) {
            Ok(fd) => return Ok((depth, Some(fd))),
            Err(error) if super::safe_read::is_not_found(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok((0, None))
}

/// Convenience wrapper: open the dirfd, perform a single locked write, drop the dirfd.
pub fn safe_locked_write(work_dir: &Path, relpath: &Path, content: &[u8]) -> Result<()> {
    let dirfd = safe_open_dirfd(work_dir)?;
    safe_locked_write_in_workdir(dirfd.as_raw_fd(), relpath, content)
}

/// Convenience wrapper: open the dirfd, perform a single locked write of a string.
pub fn safe_locked_write_str(work_dir: &Path, relpath: &Path, content: &str) -> Result<()> {
    safe_locked_write(work_dir, relpath, content.as_bytes())
}

/// Convenience: append a chunk to a file under `work_dir`.
pub fn safe_append(work_dir: &Path, relpath: &Path, content: &[u8]) -> Result<()> {
    let dirfd = safe_open_dirfd(work_dir)?;
    safe_append_in_workdir(dirfd.as_raw_fd(), relpath, content)
}

/// Convenience: create a new file under `work_dir`, failing if it exists.
pub fn safe_create_new(work_dir: &Path, relpath: &Path, content: &[u8]) -> Result<()> {
    let dirfd = safe_open_dirfd(work_dir)?;
    safe_create_new_in_workdir(dirfd.as_raw_fd(), relpath, content)
}

/// Convenience: create a directory tree under `work_dir`.
pub fn safe_create_dir_all(work_dir: &Path, relpath: &Path, mode: libc::mode_t) -> Result<()> {
    let dirfd = safe_open_dirfd(work_dir)?;
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), relpath, mode)
}

#[cfg(test)]
#[path = "safe_fs_write_tests.rs"]
mod write_tests;

#[cfg(test)]
#[path = "safe_fs_tests.rs"]
mod tests;
