//! Safe filesystem helpers for `.work/` writes that refuse to follow symlinks.
//!
//! All operations are anchored at a directory file descriptor (dirfd) opened
//! with `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, then traverse path components
//! with explicit `O_NOFOLLOW` at every level so that an attacker who can plant
//! a symlink anywhere along the path cannot redirect a `.work/` write to an
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

/// Maximum bytes accepted by `safe_locked_write_in_workdir`, `safe_append_in_workdir`,
/// `safe_create_new_in_workdir`, and `safe_write_with_mode_in_workdir`. Each
/// rejects writes larger than this and returns an error so that an unbounded
/// log capture cannot fill the `.work/` filesystem.
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

/// Try `openat2` once with the given flags/mode; returns `Ok(Some(fd))` on
/// success, `Ok(None)` if the kernel does not support `openat2` (ENOSYS /
/// EINVAL on the syscall itself, not on path resolution), or `Err` for any
/// other failure (including ELOOP, EACCES, ENOENT, EXDEV from RESOLVE_BENEATH).
#[cfg(target_os = "linux")]
fn try_openat2(dirfd: RawFd, path: &[u8], flags: i32, mode: u32) -> Result<Option<RawFd>> {
    // Quick negative cache to avoid repeated ENOSYS syscalls.
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
        // SAFETY: cur is a valid dirfd; c is NUL-terminated.
        let fd = unsafe { libc::openat(cur, c.as_ptr(), flags, mode) };
        if fd < 0 {
            let err = io::Error::last_os_error();
            let path = String::from_utf8_lossy(comp).into_owned();
            return Err(err).with_context(|| format!("safe_fs: openat failed on '{}'", path));
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
fn open_safely(dirfd: RawFd, relpath: &Path, flags: i32, mode: u32) -> Result<OwnedFd> {
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
                    format!(
                        "safe_fs: mkdirat failed on '{}'",
                        String::from_utf8_lossy(comp)
                    )
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
                    String::from_utf8_lossy(comp)
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
/// refusing every symlink along the path. The file is truncated and rewritten
/// atomically (within the flock).
///
/// Mirrors the open → lock → truncate → write → flush sequence in
/// `fs/locking.rs::locked_write`.
pub fn safe_locked_write_in_workdir(dirfd: RawFd, relpath: &Path, content: &[u8]) -> Result<()> {
    let content = enforce_size_limit(content)?;
    let fd = open_safely(
        dirfd,
        relpath,
        libc::O_WRONLY | libc::O_CREAT,
        0o600,
    )?;
    flock_exclusive(&fd)?;
    // Truncate AFTER the lock (TOCTOU prevention — see fs/locking.rs:88).
    if unsafe { libc::ftruncate(fd.as_raw_fd(), 0) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: ftruncate failed on {}", relpath.display()));
    }
    write_all_at(&fd, content, relpath)?;
    // fsync to ensure data hits disk before flock releases.
    if unsafe { libc::fsync(fd.as_raw_fd()) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: fsync failed on {}", relpath.display()));
    }
    Ok(())
}

/// Append `content` to `relpath` under `dirfd` in a single `write(2)` call
/// while holding an exclusive flock. On filesystems that honour atomic
/// `O_APPEND` semantics this is also racefree against concurrent appenders
/// from other processes; the explicit flock additionally serialises
/// in-process concurrent appenders.
pub fn safe_append_in_workdir(dirfd: RawFd, relpath: &Path, content: &[u8]) -> Result<()> {
    let content = enforce_size_limit(content)?;
    let fd = open_safely(
        dirfd,
        relpath,
        libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT,
        0o600,
    )?;
    flock_exclusive(&fd)?;
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

/// Like `safe_locked_write_in_workdir` but sets the file mode via `fchmod`
/// after open. Used for wrapper scripts that must be executable (0o755).
pub fn safe_write_with_mode_in_workdir(
    dirfd: RawFd,
    relpath: &Path,
    content: &[u8],
    mode: libc::mode_t,
) -> Result<()> {
    let content = enforce_size_limit(content)?;
    let fd = open_safely(
        dirfd,
        relpath,
        libc::O_WRONLY | libc::O_CREAT,
        mode,
    )?;
    flock_exclusive(&fd)?;
    if unsafe { libc::ftruncate(fd.as_raw_fd(), 0) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: ftruncate failed on {}", relpath.display()));
    }
    write_all_at(&fd, content, relpath)?;
    // Force the requested mode regardless of umask.
    if unsafe { libc::fchmod(fd.as_raw_fd(), mode) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("safe_fs: fchmod failed on {}", relpath.display()));
    }
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
        return Err(err).with_context(|| format!("safe_fs: unlinkat failed on {}", relpath.display()));
    }
    Ok(())
}

fn flock_exclusive(fd: &OwnedFd) -> Result<()> {
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

/// Forces the cached openat2 support flag — used in tests to exercise both
/// kernel and portable paths.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn force_disable_openat2_for_tests() {
    OPENAT2_SUPPORT.store(2, Ordering::Relaxed);
}

#[cfg(all(test, not(target_os = "linux")))]
pub(crate) fn force_disable_openat2_for_tests() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn read_file(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }

    #[test]
    fn rejects_absolute_path() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("/etc/passwd"), b"x")
            .unwrap_err();
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        let err =
            safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("../escape"), b"x").unwrap_err();
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn rejects_final_symlink() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("target.txt");
        std::fs::write(&target, b"original").unwrap();
        let link = tmp.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("link.txt"), b"new")
            .unwrap_err();
        let s = format!("{:#}", err);
        // ELOOP from O_NOFOLLOW or RESOLVE_NO_SYMLINKS rejection.
        assert!(
            s.contains("symbolic link")
                || s.contains("Too many levels of symbolic links")
                || s.contains("ELOOP")
                || s.contains("loop"),
            "expected symlink rejection, got: {s}"
        );
        // Target must remain untouched.
        assert_eq!(read_file(&target), b"original");
    }

    #[test]
    fn rejects_intermediate_symlink() {
        let tmp = TempDir::new().unwrap();
        let real_dir = tmp.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();
        let link_dir = tmp.path().join("link");
        std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        let err =
            safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("link/file.txt"), b"x")
                .unwrap_err();
        let s = format!("{:#}", err);
        assert!(
            s.contains("symbolic link")
                || s.contains("Too many levels of symbolic links")
                || s.contains("ELOOP")
                || s.contains("loop")
                || s.contains("EXDEV")
                || s.contains("not permitted"),
            "expected intermediate-symlink rejection, got: {s}"
        );
    }

    #[test]
    fn rejects_intermediate_symlink_portable_path() {
        // Force-disable openat2 to exercise the portable openat-walk path.
        let tmp = TempDir::new().unwrap();
        let real_dir = tmp.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();
        let link_dir = tmp.path().join("link");
        std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();

        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        force_disable_openat2_for_tests();
        let err =
            safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("link/file.txt"), b"x")
                .unwrap_err();
        let s = format!("{:#}", err);
        assert!(
            s.contains("symbolic link")
                || s.contains("Too many levels of symbolic links")
                || s.contains("ELOOP")
                || s.contains("loop")
                || s.contains("not a directory"),
            "expected intermediate-symlink rejection (portable), got: {s}"
        );
    }

    #[test]
    fn safe_locked_write_then_read() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("a.txt"), b"hello").unwrap();
        assert_eq!(read_file(&tmp.path().join("a.txt")), b"hello");
        safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("a.txt"), b"world!").unwrap();
        assert_eq!(read_file(&tmp.path().join("a.txt")), b"world!");
    }

    #[test]
    fn safe_locked_write_concurrent_serialises() {
        use std::sync::Arc;
        use std::thread;
        let tmp = Arc::new(TempDir::new().unwrap());
        let path = tmp.path().join("c.txt");
        // Pre-create so all threads race on the same inode.
        std::fs::write(&path, b"").unwrap();

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let tmp = Arc::clone(&tmp);
                thread::spawn(move || {
                    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
                    let content = format!("payload-{i}").into_bytes();
                    safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("c.txt"), &content)
                        .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let out = read_file(&path);
        let s = String::from_utf8_lossy(&out);
        assert!(s.starts_with("payload-"), "{s}");
    }

    #[test]
    fn safe_append_preserves_order() {
        use std::sync::Arc;
        use std::thread;
        let tmp = Arc::new(TempDir::new().unwrap());

        let handles: Vec<_> = (0..16)
            .map(|i| {
                let tmp = Arc::clone(&tmp);
                thread::spawn(move || {
                    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
                    let line = format!("line-{i:02}\n");
                    safe_append_in_workdir(dirfd.as_raw_fd(), Path::new("log.txt"), line.as_bytes())
                        .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let out = read_file(&tmp.path().join("log.txt"));
        let s = String::from_utf8_lossy(&out);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 16, "expected 16 lines, got {}", lines.len());
        // No torn writes: every line must be of the form "line-NN".
        for l in &lines {
            assert!(l.starts_with("line-"), "torn line: {l:?}");
        }
    }

    #[test]
    fn safe_create_new_refuses_existing() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        safe_create_new_in_workdir(dirfd.as_raw_fd(), Path::new("h.txt"), b"first").unwrap();
        let err =
            safe_create_new_in_workdir(dirfd.as_raw_fd(), Path::new("h.txt"), b"second").unwrap_err();
        let s = format!("{:#}", err);
        assert!(
            s.contains("exists") || s.contains("EEXIST"),
            "expected EEXIST, got: {s}"
        );
        assert_eq!(read_file(&tmp.path().join("h.txt")), b"first");
    }

    #[test]
    fn safe_write_with_mode_sets_bits() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        safe_write_with_mode_in_workdir(
            dirfd.as_raw_fd(),
            Path::new("script.sh"),
            b"#!/bin/sh\necho ok\n",
            0o755,
        )
        .unwrap();
        let meta = std::fs::metadata(tmp.path().join("script.sh")).unwrap();
        let perm = meta.permissions().mode() & 0o777;
        assert_eq!(perm, 0o755, "got mode {:o}", perm);
    }

    #[test]
    fn safe_create_dir_all_creates_nested() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new("a/b/c"), 0o755).unwrap();
        assert!(tmp.path().join("a/b/c").is_dir());
        // Idempotent.
        safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new("a/b/c"), 0o755).unwrap();
    }

    #[test]
    fn safe_create_dir_all_rejects_intermediate_symlink() {
        let tmp = TempDir::new().unwrap();
        let real_dir = tmp.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();
        let link_dir = tmp.path().join("link");
        std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        let err = safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new("link/inner"), 0o755)
            .unwrap_err();
        let s = format!("{:#}", err);
        assert!(
            s.contains("symbolic link")
                || s.contains("Too many levels of symbolic links")
                || s.contains("ELOOP")
                || s.contains("loop")
                || s.contains("File exists")
                || s.contains("EEXIST"),
            "expected symlink rejection or EEXIST race, got: {s}"
        );
    }

    #[test]
    fn rejects_oversize_write() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        let too_big = vec![b'x'; MAX_LOG_BYTES + 1];
        let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("big.txt"), &too_big)
            .unwrap_err();
        assert!(err.to_string().contains("refusing write"));
    }

    #[test]
    fn safe_remove_drops_file() {
        let tmp = TempDir::new().unwrap();
        let dirfd = safe_open_dirfd(tmp.path()).unwrap();
        safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("d.txt"), b"bye").unwrap();
        safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new("d.txt")).unwrap();
        assert!(!tmp.path().join("d.txt").exists());
        // Removing again returns Ok (idempotent for ENOENT).
        safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new("d.txt")).unwrap();
    }
}

