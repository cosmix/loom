//! Bounded, dirfd-relative reads that reject symlinks at every path component.

use anyhow::{bail, Context, Result};
use nix::dir::{Dir, Entry, Type};
use nix::fcntl::{AtFlags, OFlag};
use nix::sys::stat::{fstatat, Mode};
use std::ffi::{OsStr, OsString};
use std::fs::{File, Metadata};
use std::io;
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::safe_fs::{open_safely, safe_open_dirfd};

mod tail;
pub(crate) use tail::read_tail;

/// Whether any cause in `error`'s chain is `ENOENT`: an absent file, an
/// absent parent directory, or an absent dirfd root. Shared by every no-follow
/// open that treats a missing path as `Ok(None)` rather than an error.
pub(crate) fn is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|io_error| io_error.kind() == io::ErrorKind::NotFound)
    })
}

/// fstat an already-opened file and refuse anything that is not a regular
/// file. Shared by `read_bounded` and `open_regular_no_follow`, which both
/// resolve through a no-follow dirfd walk and need the same check on the fd
/// they get back.
fn regular_file_metadata(file: &File) -> Result<Metadata> {
    let metadata = file.metadata().context("Failed to inspect opened file")?;
    if !metadata.is_file() {
        bail!("opened path is not a regular file");
    }
    Ok(metadata)
}

/// Reject files whose inode can also be modified through another name.
pub(crate) fn single_link_regular_file(file: &File) -> Result<()> {
    if regular_file_metadata(file)?.nlink() != 1 {
        bail!("opened path is a hard link with more than one name");
    }
    Ok(())
}

/// Read at most `max_bytes` from a regular file beneath `root`.
///
/// `relative` must not be absolute or contain `..`. The kernel opens every
/// component with no-follow semantics, so an attacker cannot substitute a
/// symlink between validation and use. `O_NONBLOCK` keeps a FIFO put in the
/// file's place from blocking the open before the regular-file check refuses
/// it; it has no effect on reading a regular file.
pub fn read_bounded(root: &Path, relative: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let root_fd = safe_open_dirfd(root)?;
    read_bounded_at(&root_fd, relative, max_bytes)
}

/// Bounded read anchored to an already-open directory, without taking a lock.
pub(crate) fn read_bounded_at(
    root_fd: &OwnedFd,
    relative: &Path,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let flags = libc::O_RDONLY | libc::O_NONBLOCK;
    let file_fd = open_safely(root_fd.as_raw_fd(), relative, flags, 0)
        .with_context(|| format!("Refusing unsafe read of {}", relative.display()))?;
    read_open_bounded(File::from(file_fd), relative, max_bytes)
}

/// Bounded read of an already-open regular file. The size comes from `fstat`
/// on the same descriptor that is read, so nothing can be swapped in between;
/// `relative` only names the file in errors. A file over `max_bytes`, by its
/// size or by growing while read, yields a typed [`OverLimit`].
pub(crate) fn read_open_bounded(file: File, relative: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let metadata = regular_file_metadata(&file)
        .with_context(|| format!("Refusing unsafe read of {}", relative.display()))?;
    if metadata.len() > max_bytes as u64 {
        return Err(OverLimit::new(relative, max_bytes, OverLimitStage::Stat).into());
    }
    read_capped(file, metadata.len() as usize, relative, max_bytes)
}

/// Read `source` to its end, refusing more than `max_bytes`. One byte past
/// the limit is enough to tell that the file grew after its size was checked.
fn read_capped(
    source: impl Read,
    capacity: usize,
    relative: &Path,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(capacity);
    let limit = u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut reader = source.take(limit);
    reader
        .read_to_end(&mut bytes)
        .context("Failed to read opened file")?;
    if bytes.len() > max_bytes {
        return Err(OverLimit::new(relative, max_bytes, OverLimitStage::Read).into());
    }
    Ok(bytes)
}

/// When `read_bounded` noticed a file over its limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverLimitStage {
    /// The size reported by `fstat` already exceeded the limit.
    Stat,
    /// The file grew past the limit while it was being read.
    Read,
}

/// `read_bounded` refused a file for exceeding its byte limit. Callers that
/// treat an oversized file as a changed file match it with `downcast_ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverLimit {
    path: String,
    max_bytes: usize,
    stage: OverLimitStage,
}

impl OverLimit {
    fn new(relative: &Path, max_bytes: usize, stage: OverLimitStage) -> Self {
        Self {
            path: relative.display().to_string(),
            max_bytes,
            stage,
        }
    }
}

impl std::fmt::Display for OverLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.stage {
            OverLimitStage::Stat => {
                write!(
                    f,
                    "{} exceeds the {} byte verification limit",
                    self.path, self.max_bytes
                )
            }
            OverLimitStage::Read => write!(
                f,
                "{} grew beyond the {} byte verification limit while reading",
                self.path, self.max_bytes
            ),
        }
    }
}

impl std::error::Error for OverLimit {}

/// Read a bounded UTF-8 file beneath `root` without following symlinks.
pub fn read_to_string_bounded(root: &Path, relative: &Path, max_bytes: usize) -> Result<String> {
    let bytes = read_bounded(root, relative, max_bytes)?;
    String::from_utf8(bytes).with_context(|| format!("{} is not valid UTF-8", relative.display()))
}

/// Open `relpath` beneath `root` without following a symlink at any path
/// component, refusing anything that is not a regular file with a single
/// link. Shared by the memory, telemetry, and stage-request spools: each
/// sits inside a sandboxed session's write boundary, so the session can swap
/// the spool or its parent directory for a symlink, a hard link, or a FIFO -
/// following any of those would let it redirect the trusted daemon's read or
/// truncate outside the worktree. `O_NONBLOCK` keeps a FIFO from blocking the
/// open before the regular-file check runs, and has no effect on a regular
/// file.
///
/// `Ok(None)` when `root`, an intermediate directory, or `relpath` itself is
/// absent - the common case on every daemon poll tick - so callers need no
/// separate existence check. Never creates anything.
pub(crate) fn open_regular_no_follow(
    root: &Path,
    relpath: &str,
    flags: i32,
) -> Result<Option<File>> {
    let opened = safe_open_dirfd(root).and_then(|root_fd| {
        open_safely(
            root_fd.as_raw_fd(),
            Path::new(relpath),
            flags | libc::O_NONBLOCK,
            0,
        )
    });
    let file = match opened {
        Ok(descriptor) => File::from(descriptor),
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error.context("no-follow open failed")),
    };
    single_link_regular_file(&file)?;
    Ok(Some(file))
}

/// Whether `path` is itself a regular file: a symlink, even to a regular file,
/// is not. A pre-check for listings; a read whose answer must hold at open
/// time uses [`open_regular_no_follow`].
pub(crate) fn is_regular_no_follow(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

/// What a directory entry is in itself: a symlink is a `Symlink`, never what
/// it points to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Directory,
    File,
    Symlink,
    /// A FIFO, socket, or character or block device.
    Special,
}

/// The entries, without `.` and `..`, of the directory `relative` beneath the
/// directory `root`, or of `root` itself when `relative` is empty.
///
/// The directory is opened from `root` with a symlink refused at every
/// component ([`open_safely`]) and listed from that descriptor, so a
/// directory swapped for a symlink before the open fails it (`ELOOP`, or
/// `ENOTDIR` on macOS) instead of listing the symlink's target. `O_NONBLOCK`
/// keeps a FIFO swapped in from blocking the open before `O_DIRECTORY`
/// refuses it.
pub(crate) fn list_dir_no_follow(
    root: &OwnedFd,
    relative: &Path,
) -> Result<Vec<(OsString, EntryKind)>> {
    let mut listing = if relative.as_os_str().is_empty() {
        let flags = OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_CLOEXEC;
        Dir::openat(root, ".", flags, Mode::empty())?
    } else {
        let flags = libc::O_DIRECTORY | libc::O_RDONLY | libc::O_NONBLOCK;
        Dir::from_fd(open_safely(root.as_raw_fd(), relative, flags, 0)?)?
    };
    let entries = listing
        .iter()
        .collect::<nix::Result<Vec<Entry>>>()
        .context("cannot read the directory")?;
    entries
        .iter()
        .filter(|entry| !matches!(entry.file_name().to_bytes(), b"." | b".."))
        .map(|entry| {
            let name = OsStr::from_bytes(entry.file_name().to_bytes()).to_os_string();
            Ok((name, entry_kind(&listing, entry)?))
        })
        .collect()
}

/// The entry's type from the listing, or from an `fstatat` that does not
/// follow a symlink where the filesystem leaves it unknown (`DT_UNKNOWN`).
fn entry_kind(listing: &Dir, entry: &Entry) -> Result<EntryKind> {
    let kind = match entry.file_type() {
        Some(Type::Directory) => EntryKind::Directory,
        Some(Type::File) => EntryKind::File,
        Some(Type::Symlink) => EntryKind::Symlink,
        Some(_) => EntryKind::Special,
        None => {
            let stat = fstatat(listing, entry.file_name(), AtFlags::AT_SYMLINK_NOFOLLOW)
                .with_context(|| format!("cannot inspect {:?}", entry.file_name()))?;
            kind_of_mode(stat.st_mode)
        }
    };
    Ok(kind)
}

/// The kind a no-follow `st_mode` describes: a symlink is a `Symlink`, never its target.
pub(crate) fn kind_of_mode(mode: libc::mode_t) -> EntryKind {
    match mode & libc::S_IFMT {
        libc::S_IFDIR => EntryKind::Directory,
        libc::S_IFREG => EntryKind::File,
        libc::S_IFLNK => EntryKind::Symlink,
        _ => EntryKind::Special,
    }
}

#[cfg(test)]
mod tests;
