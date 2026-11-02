//! Remove only unpublished staging trees using descriptor-relative operations.
use anyhow::{Context, Result};
use std::ffi::{CStr, CString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;

use crate::fs::safe_fs::safe_open_dirfd;

pub(super) fn cleanup(dir: &Path) -> Result<()> {
    let root = safe_open_dirfd(dir)?;
    let name = c"staging";
    // SAFETY: root is an open descriptor and name is NUL-terminated.
    let fd = unsafe {
        libc::openat(
            root.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            return Ok(());
        }
        return Err(error).context("open generation staging directory");
    }
    // SAFETY: openat returned an owned descriptor.
    let staging = unsafe { OwnedFd::from_raw_fd(fd) };
    remove_contents(&staging)
}

struct Directory(*mut libc::DIR);
impl Drop for Directory {
    fn drop(&mut self) {
        // SAFETY: fdopendir returned this stream and it has not been closed.
        unsafe {
            libc::closedir(self.0);
        }
    }
}

fn remove_contents(dir: &OwnedFd) -> Result<()> {
    let duplicate = dir.try_clone()?;
    use std::os::fd::IntoRawFd;
    // SAFETY: duplicate is transferred to fdopendir on success.
    let stream = unsafe { libc::fdopendir(duplicate.as_raw_fd()) };
    if stream.is_null() {
        return Err(std::io::Error::last_os_error()).context("read staging directory");
    }
    let _ = duplicate.into_raw_fd();
    let stream = Directory(stream);
    loop {
        // SAFETY: stream remains valid until its guard is dropped.
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            break;
        }
        // SAFETY: readdir returns a NUL-terminated name valid until the next call.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        remove_entry(dir, &CString::new(name.to_bytes())?)?;
    }
    Ok(())
}

fn remove_entry(parent: &OwnedFd, name: &CStr) -> Result<()> {
    // SAFETY: parent is open and name is NUL-terminated; O_NOFOLLOW refuses links.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    let flags = if fd >= 0 {
        // SAFETY: openat returned an owned descriptor.
        let child = unsafe { OwnedFd::from_raw_fd(fd) };
        remove_contents(&child)?;
        libc::AT_REMOVEDIR
    } else {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            return Ok(());
        }
        if error.raw_os_error() != Some(libc::ENOTDIR) && error.raw_os_error() != Some(libc::ELOOP)
        {
            return Err(error).context("open staged entry");
        }
        0
    };
    // SAFETY: parent and name are valid; unlinkat never follows the final component.
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), flags) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error).context("remove staged entry");
        }
    }
    Ok(())
}
