//! Safe filesystem helpers for `.loom/work/` writes that refuse to follow symlinks.
//!
//! All operations are anchored at a directory file descriptor (dirfd) opened
//! with `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, then traverse path components
//! with explicit `O_NOFOLLOW` at every level so that an attacker who can plant
//! a symlink anywhere along the path cannot redirect a `.loom/work/` write to an
//! arbitrary file (`MN8`).
//!
//! Linux preferred path uses the `openat2(2)` syscall with
//! `RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS` for atomic,
//! kernel-enforced safety. When `openat2` is unavailable (older Linux, macOS,
//! Apple Container) the portable component-by-component openat walk takes
//! over, refusing intermediate or terminal symlinks via `O_NOFOLLOW`.
//!
//! Universally refused: paths containing `..` components, absolute paths,
//! symlinks at any segment of the resolved path.

use anyhow::{bail, Context, Result};
use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Component, Path};
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU8, Ordering};

/// Maximum bytes accepted in one write or append call. Append callers must
/// enforce any total-file or retention bound separately.
pub const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;

// openat2 syscall number and resolve flags (Linux, kernel 5.6+).
#[cfg(target_os = "linux")]
const SYS_OPENAT2: libc::c_long = 437;
#[cfg(target_os = "linux")]
const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
#[cfg(target_os = "linux")]
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
#[cfg(target_os = "linux")]
const RESOLVE_BENEATH: u64 = 0x08;

#[cfg(target_os = "linux")]
#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

// Cached probe of openat2 kernel support: 0 = unknown, 1 = supported, 2 = unsupported.
#[cfg(target_os = "linux")]
static OPENAT2_SUPPORT: AtomicU8 = AtomicU8::new(0);

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

/// Try `openat2` once: `Ok(Some(fd))` on success, `Ok(None)` when the kernel
/// lacks it or seccomp blocks it (ENOSYS / EPERM, cached process-wide) or a
/// test thread forced the portable path, `Err` for any other failure.
#[cfg(target_os = "linux")]
fn try_openat2(dirfd: RawFd, path: &[u8], flags: i32, mode: u32) -> Result<Option<RawFd>> {
    #[cfg(test)]
    if FORCE_PORTABLE.get() {
        return Ok(None);
    }
    if OPENAT2_SUPPORT.load(Ordering::Relaxed) == 2 {
        return Ok(None);
    }
    let c = path_to_cstring(path)?;
    let how = OpenHow {
        flags: flags as u64,
        mode: mode as u64,
        resolve: RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS,
    };
    // SAFETY: openat2 is a stable syscall; we pass a well-formed open_how.
    let ret = unsafe {
        libc::syscall(
            SYS_OPENAT2,
            dirfd,
            c.as_ptr(),
            &how as *const _,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if ret >= 0 {
        if OPENAT2_SUPPORT.load(Ordering::Relaxed) == 0 {
            OPENAT2_SUPPORT.store(1, Ordering::Relaxed);
        }
        return Ok(Some(ret as RawFd));
    }
    let err = io::Error::last_os_error();
    let raw = err.raw_os_error().unwrap_or(0);
    if raw == libc::ENOSYS || raw == libc::EPERM {
        // Kernel does not support openat2 (older than 5.6) or seccomp blocks
        // it; fall through to the portable path.
        OPENAT2_SUPPORT.store(2, Ordering::Relaxed);
        return Ok(None);
    }
    Err(err).context("safe_fs: openat2 failed")
}

#[cfg(not(target_os = "linux"))]
fn try_openat2(_dirfd: RawFd, _path: &[u8], _flags: i32, _mode: u32) -> Result<Option<RawFd>> {
    Ok(None)
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
/// symlink along the path. Tries `openat2` first; falls back to the portable
/// openat-walk if unsupported.
pub(crate) fn open_safely(dirfd: RawFd, relpath: &Path, flags: i32, mode: u32) -> Result<OwnedFd> {
    let components = validate_relpath(relpath)?;

    // Linux fast path: single syscall, kernel-enforced.
    let joined: Vec<u8> = {
        let mut buf = Vec::new();
        for (i, c) in components.iter().enumerate() {
            if i > 0 {
                buf.push(b'/');
            }
            buf.extend_from_slice(c);
        }
        buf
    };
    if let Some(fd) = try_openat2(
        dirfd,
        &joined,
        flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        mode,
    )? {
        // SAFETY: openat2 returned a valid fd we now own.
        return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
    }

    let fd = portable_open_walk(dirfd, &components, flags, mode)?;
    // SAFETY: portable walk returned a valid fd we now own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Open a directory (intermediate) under `dirfd`, refusing every symlink.
fn open_dir_safely(dirfd: RawFd, relpath: &Path) -> Result<OwnedFd> {
    let flags = libc::O_DIRECTORY | libc::O_RDONLY;
    open_safely(dirfd, relpath, flags, 0)
}

/// Create every component of `relpath` as a directory under `dirfd`,
/// refusing to traverse any symlink. Idempotent: existing directories are
/// accepted, existing non-directories are rejected.
pub fn safe_create_dir_all_in_workdir(
    dirfd: RawFd,
    relpath: &Path,
    mode: libc::mode_t,
) -> Result<()> {
    let components = validate_relpath(relpath)?;
    let mut cur: RawFd = dirfd;
    let mut owned: Option<OwnedFd> = None;
    for comp in &components {
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
    let fd = open_locked_regular(
        dirfd,
        relpath,
        libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT,
        0o600,
    )?;
    write_all_at(&fd, content, relpath)?;
    Ok(())
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
    let relative = target.strip_prefix(project_root).with_context(|| {
        format!(
            "safe_fs: refusing replace — target {} not under project_root {}",
            target.display(),
            project_root.display()
        )
    })?;
    let root = project_root
        .canonicalize()
        .context("cannot resolve project root")?;
    super::safe_replace::replace_private(&root, relative, content)
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

fn open_locked_regular(dirfd: RawFd, path: &Path, flags: i32, mode: u32) -> Result<OwnedFd> {
    let fd = open_safely(dirfd, path, flags | libc::O_NONBLOCK, mode)?;
    let file = std::fs::File::from(fd);
    super::safe_read::single_link_regular_file(&file)?;
    let fd = OwnedFd::from(file);
    flock_exclusive(&fd)?;
    let file = std::fs::File::from(fd);
    super::safe_read::single_link_regular_file(&file)?;
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
thread_local!(static FORCE_PORTABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
/// Forces the portable path on the calling test thread only, so it never leaks into other tests.
#[cfg(test)]
pub(crate) fn force_disable_openat2_for_tests() {
    FORCE_PORTABLE.set(true);
}

#[cfg(test)]
#[path = "safe_fs_write_tests.rs"]
mod write_tests;

#[cfg(test)]
#[path = "safe_fs_tests.rs"]
mod tests;
