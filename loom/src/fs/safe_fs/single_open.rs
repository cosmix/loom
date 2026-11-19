//! The single-syscall fast path of [`super::open_safely`]: one kernel-enforced open of a whole
//! relative path that refuses a symlink in any component without opening the intermediate
//! directories one by one.
//!
//! Linux uses `openat2(2)` with `RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`.
//! macOS uses `openat(2)` with `O_NOFOLLOW_ANY`. Not opening the intermediates matters there: a
//! planning session's Seatbelt profile denies reading the planning root and allows only some
//! paths below it, so opening a denied intermediate directory fails with `EPERM` while one
//! no-follow open of an allowed path succeeds. `..` never reaches this path (`validate_relpath`
//! refuses it), so a symlink-free path stays beneath the anchor without `RESOLVE_BENEATH`.

use anyhow::{Context, Result};
use std::io;
use std::os::unix::io::RawFd;

use super::path_to_cstring;

#[cfg(test)]
thread_local!(static FORCE_PORTABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });

/// Forces the portable walk on the calling test thread only, so it never leaks into other tests.
#[cfg(test)]
pub(crate) fn force_disable_openat2_for_tests() {
    FORCE_PORTABLE.set(true);
}

#[cfg(test)]
fn forced_portable() -> bool {
    FORCE_PORTABLE.get()
}

#[cfg(not(test))]
fn forced_portable() -> bool {
    false
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    // openat2 syscall number and resolve flags (Linux, kernel 5.6+).
    const SYS_OPENAT2: libc::c_long = 437;
    const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
    const RESOLVE_NO_SYMLINKS: u64 = 0x04;
    const RESOLVE_BENEATH: u64 = 0x08;

    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }

    // Cached probe of openat2 kernel support: 0 = unknown, 1 = supported, 2 = unsupported.
    static OPENAT2_SUPPORT: AtomicU8 = AtomicU8::new(0);

    /// Try `openat2` once: `Ok(Some(fd))` on success, `Ok(None)` when the kernel lacks it or
    /// seccomp blocks it (ENOSYS / EPERM, cached process-wide), `Err` for any other failure.
    pub(super) fn try_open(
        dirfd: RawFd,
        path: &[u8],
        flags: i32,
        mode: u32,
    ) -> Result<Option<RawFd>> {
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
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    /// First Darwin major release whose kernel knows `O_NOFOLLOW_ANY` (macOS 11).
    const NOFOLLOW_ANY_DARWIN: u32 = 20;

    // Cached support: 0 = unknown, 1 = supported, 2 = unsupported.
    static NOFOLLOW_ANY_SUPPORT: AtomicU8 = AtomicU8::new(0);

    /// Whether the running kernel honours `O_NOFOLLOW_ANY`. Checked from the release rather
    /// than by trying the flag, because a kernel that predates a flag may ignore the bit
    /// instead of failing, which would silently follow symlinks.
    fn supported() -> bool {
        match NOFOLLOW_ANY_SUPPORT.load(Ordering::Relaxed) {
            1 => return true,
            2 => return false,
            _ => {}
        }
        let ok = darwin_major().is_some_and(|major| major >= NOFOLLOW_ANY_DARWIN);
        NOFOLLOW_ANY_SUPPORT.store(if ok { 1 } else { 2 }, Ordering::Relaxed);
        ok
    }

    fn darwin_major() -> Option<u32> {
        // SAFETY: utsname is plain data; uname fills it and NUL-terminates `release`.
        let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
        // SAFETY: `uts` is a valid, writable utsname.
        if unsafe { libc::uname(&mut uts) } != 0 {
            return None;
        }
        // SAFETY: uname succeeded, so `release` holds a NUL-terminated string.
        let release = unsafe { std::ffi::CStr::from_ptr(uts.release.as_ptr()) };
        release.to_str().ok()?.split('.').next()?.parse().ok()
    }

    /// Try one `openat` with `O_NOFOLLOW_ANY`: `Ok(Some(fd))` on success, `Ok(None)` when the
    /// kernel predates the flag or rejects it (`EINVAL`), `Err` for any other failure,
    /// including `ELOOP` for a symlink in any component.
    ///
    /// `O_NOFOLLOW` is dropped: the kernel refuses it alongside `O_NOFOLLOW_ANY` with `EINVAL`,
    /// and `O_NOFOLLOW_ANY` already refuses a symlink at the last component. An `EINVAL` is not
    /// cached, so one rejected flag combination never turns the fast path off for the process.
    pub(super) fn try_open(
        dirfd: RawFd,
        path: &[u8],
        flags: i32,
        mode: u32,
    ) -> Result<Option<RawFd>> {
        if !supported() {
            return Ok(None);
        }
        let c = path_to_cstring(path)?;
        let fd = super::super::openat_retrying_create_race(
            dirfd,
            &c,
            (flags & !libc::O_NOFOLLOW) | libc::O_NOFOLLOW_ANY,
            mode,
        );
        if fd >= 0 {
            return Ok(Some(fd));
        }
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINVAL) {
            return Ok(None);
        }
        let shown = path.escape_ascii();
        Err(err).with_context(|| format!("safe_fs: openat failed on '{shown}'"))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use super::{RawFd, Result};

    pub(super) fn try_open(
        _dirfd: RawFd,
        _path: &[u8],
        _flags: i32,
        _mode: u32,
    ) -> Result<Option<RawFd>> {
        Ok(None)
    }
}

/// Open `path` (validated, components joined by `/`) under `dirfd` in one kernel-enforced call
/// that refuses every symlink. `Ok(None)` means the platform lacks such a call (or a test
/// thread forced the portable path) and the caller walks component by component instead.
pub(super) fn try_single_open(
    dirfd: RawFd,
    path: &[u8],
    flags: i32,
    mode: u32,
) -> Result<Option<RawFd>> {
    if forced_portable() {
        return Ok(None);
    }
    imp::try_open(dirfd, path, flags, mode)
}

#[cfg(test)]
#[path = "single_open_tests.rs"]
mod tests;
