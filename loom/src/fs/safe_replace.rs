//! Atomic replacement of an existing regular file without following symlinks.
//!
//! Sessions can write `doc/`, so a path the server saves to may have become a symlink, or
//! gained a symlinked directory, since it was checked. The parent directory is opened with the
//! no-follow walk and every later step works on that one directory fd: the target check, the
//! temporary file, the rename and the directory sync. A target swapped for a symlink between
//! the check and the rename is replaced as a link entry, so the link's target is never written.

use std::ffi::{CString, OsStr};
use std::fs::{File, Permissions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};

use super::safe_fs::{open_safely, safe_open_dirfd};

#[cfg(test)]
#[path = "safe_replace_tests.rs"]
mod tests;

#[cfg(test)]
thread_local! {
    static FAIL_DIR_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Replace the regular file `rel` (relative to `root`, no `..`) with `bytes`, keeping its
/// permission bits. Refuses a missing target, a target that is not a regular file (a symlink
/// included) and a path with a symlinked directory component; never creates the target. Once
/// the rename has happened the call succeeds, even when syncing the directory then fails.
pub fn replace_no_follow(root: &Path, rel: &Path, bytes: &[u8]) -> Result<()> {
    let name = match rel.components().next_back() {
        Some(Component::Normal(name)) => name,
        _ => bail!("{} does not name a file", rel.display()),
    };
    let parent = open_parent(root, rel)?;
    let mode = target_mode(&parent, &c_name(name.as_bytes())?)
        .with_context(|| format!("refusing to replace {}", rel.display()))?;
    publish_in_dir(&parent, name, bytes, mode, true)
        .with_context(|| format!("cannot replace {}", rel.display()))?;
    Ok(())
}

/// Atomically restore a file with an explicit mode, creating it if absent.
/// Existing non-regular targets are refused; a swapped target is never opened
/// for writing because publication replaces the directory entry instead.
pub fn restore_no_follow(root: &Path, rel: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    if bytes.len() > super::safe_fs::MAX_LOG_BYTES {
        bail!("restore exceeds the maximum file size");
    }
    let name = match rel.components().next_back() {
        Some(Component::Normal(name)) => name,
        _ => bail!("{} does not name a file", rel.display()),
    };
    let parent = open_parent(root, rel)?;
    check_restore_target(&parent, &c_name(name.as_bytes())?)
        .with_context(|| format!("refusing to restore {}", rel.display()))?;
    publish_in_dir(&parent, name, bytes, mode, true)
}

/// Publish under an anchored root, creating or replacing the target with mode
/// 0600 subject to the process umask. Intermediate symlinks are refused.
pub(crate) fn replace_private(root: &Path, rel: &Path, bytes: &[u8]) -> Result<()> {
    let name = match rel.components().next_back() {
        Some(Component::Normal(name)) => name,
        _ => bail!("{} does not name a file", rel.display()),
    };
    let parent = open_parent(root, rel)?;
    publish_in_dir(&parent, name, bytes, 0o600, false)
}

fn check_restore_target(parent: &OwnedFd, name: &CString) -> Result<()> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: the parent and name are valid and stat points to writable storage.
    let rc = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(error.into())
        };
    }
    // SAFETY: successful fstatat initialized stat.
    let stat = unsafe { stat.assume_init() };
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
        bail!("the target is not a regular file");
    }
    Ok(())
}

/// Fill the temporary file `tmp` and rename it over `name`; the temporary file is removed when
/// either step fails.
fn publish(
    parent: &OwnedFd,
    tmp: &CString,
    name: &CString,
    file: File,
    mode: u32,
    force_mode: bool,
    bytes: &[u8],
) -> Result<()> {
    let written =
        fill_with_mode(file, mode, force_mode, bytes).and_then(|()| rename_at(parent, tmp, name));
    if written.is_err() {
        unlink_at(parent, tmp);
    }
    written
}

/// Publish bytes in an already opened directory; the caller owns any lock.
pub(crate) fn publish_in_dir(
    parent: &OwnedFd,
    name: &OsStr,
    bytes: &[u8],
    mode: u32,
    force_mode: bool,
) -> Result<()> {
    if let Some(error) = publish_in_dir_reporting(parent, name, bytes, mode, force_mode)? {
        tracing::warn!("cannot sync directory after publication: {error:#}");
    }
    Ok(())
}

/// Returns a sync warning separately after a successful rename; an error means
/// publication did not occur. The caller owns any synchronization lock.
pub(crate) fn publish_in_dir_reporting(
    parent: &OwnedFd,
    name: &OsStr,
    bytes: &[u8],
    mode: u32,
    force_mode: bool,
) -> Result<Option<anyhow::Error>> {
    let name = c_name(name.as_bytes())?;
    let tmp = c_name(&temp_name(name.as_bytes()))?;
    let file = create_temp(parent, &tmp, mode)?;
    publish(parent, &tmp, &name, file, mode, force_mode, bytes)?;
    Ok(sync_dir(parent).err())
}

/// The directory holding `rel`, opened from `root` without following any symlink.
fn open_parent(root: &Path, rel: &Path) -> Result<OwnedFd> {
    let root_fd = safe_open_dirfd(root)?;
    let parent = rel.parent().unwrap_or(Path::new(""));
    if parent.as_os_str().is_empty() {
        return Ok(root_fd);
    }
    let flags = libc::O_DIRECTORY | libc::O_RDONLY;
    open_safely(root_fd.as_raw_fd(), parent, flags, 0)
        .with_context(|| format!("refusing to open the directory {}", parent.display()))
}

fn c_name(bytes: &[u8]) -> Result<CString> {
    CString::new(bytes).context("a file name holds a NUL byte")
}

/// The longest file name the supported file systems accept, in bytes.
const NAME_MAX: usize = 255;
/// What `temp_name` adds around the target's name: `.`, `.`, 16 hex digits and `.tmp`.
const TEMP_OVERHEAD: usize = 1 + 1 + 16 + 4;

/// `.<name>.<16 random hex>.tmp`, with `name` cut so the result fits `NAME_MAX` (a target with
/// a name of 234 bytes or more would otherwise fail with `ENAMETOOLONG`).
fn temp_name(name: &[u8]) -> Vec<u8> {
    let random = uuid::Uuid::new_v4().simple().to_string();
    let mut tmp = b".".to_vec();
    tmp.extend_from_slice(&name[..prefix_len(name, NAME_MAX - TEMP_OVERHEAD)]);
    tmp.push(b'.');
    tmp.extend_from_slice(&random.as_bytes()[..16]);
    tmp.extend_from_slice(b".tmp");
    tmp
}

/// The length of the longest prefix of `name` within `max` bytes that does not end inside a
/// UTF-8 sequence. Bytes that are not UTF-8 are cut where they fall: the name is only a label.
fn prefix_len(name: &[u8], max: usize) -> usize {
    let mut len = max.min(name.len());
    while len > 0 && len < name.len() && name[len] & 0xC0 == 0x80 {
        len -= 1;
    }
    len
}

/// The permission bits of `name` in `parent`, which must be a regular file. The open does not
/// follow a final symlink (`ELOOP`), and `O_NONBLOCK` keeps a FIFO from blocking it.
fn target_mode(parent: &OwnedFd, name: &CString) -> Result<u32> {
    let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
    // SAFETY: `parent` is an open directory fd and `name` a NUL-terminated string.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        let error = io::Error::last_os_error();
        return Err(error).context("cannot open the target without following links");
    }
    // SAFETY: openat returned a valid fd we now own.
    let target = File::from(unsafe { OwnedFd::from_raw_fd(fd) });
    let meta = target.metadata().context("cannot stat the target")?;
    if !meta.file_type().is_file() {
        bail!("the target is not a regular file");
    }
    Ok(meta.permissions().mode() & 0o7777)
}

/// Create `tmp` in `parent`; it must not exist yet and is never reached through a link.
fn create_temp(parent: &OwnedFd, tmp: &CString, mode: u32) -> Result<File> {
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: `parent` is an open directory fd, `tmp` a NUL-terminated string, and the mode
    // argument `openat` reads for `O_CREAT` is a `u32`, the C unsigned int it expects.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), tmp.as_ptr(), flags, mode) };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: openat returned a valid fd we now own.
    Ok(File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
}

/// Write `bytes`, restore the target's mode (the umask narrowed the create), and sync.
fn fill_with_mode(mut file: File, mode: u32, force_mode: bool, bytes: &[u8]) -> Result<()> {
    file.write_all(bytes)
        .context("cannot write the temporary file")?;
    if force_mode {
        file.set_permissions(Permissions::from_mode(mode))
            .context("cannot set the temporary file's mode")?;
    }
    file.sync_all().context("cannot sync the temporary file")
}

fn rename_at(parent: &OwnedFd, from: &CString, to: &CString) -> Result<()> {
    let dir = parent.as_raw_fd();
    // SAFETY: both names are NUL-terminated and resolved in the same open directory fd.
    let rc = unsafe { libc::renameat(dir, from.as_ptr(), dir, to.as_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error()).context("cannot rename the temporary file");
    }
    Ok(())
}

fn unlink_at(parent: &OwnedFd, name: &CString) {
    // SAFETY: `name` is NUL-terminated and resolved in the open directory fd `parent`.
    let rc = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
    if rc != 0 {
        let error = io::Error::last_os_error();
        let name = Path::new(OsStr::from_bytes(name.as_bytes()));
        tracing::warn!(
            "cannot remove the temporary file {}: {error}",
            name.display()
        );
    }
}

fn sync_dir(parent: &OwnedFd) -> Result<()> {
    #[cfg(test)]
    if FAIL_DIR_SYNC.with(|flag| flag.get()) {
        bail!("injected directory sync failure");
    }
    // SAFETY: `parent` is an open fd; fsync does not take ownership of it.
    if unsafe { libc::fsync(parent.as_raw_fd()) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}
