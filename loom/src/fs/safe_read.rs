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
    let file = File::from(file_fd);
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
mod tests {
    use super::*;

    #[test]
    fn a_file_over_the_limit_yields_the_typed_error_with_its_wording() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("big"), [b'x'; 8]).unwrap();

        let error = read_bounded(root.path(), Path::new("big"), 4).unwrap_err();
        let typed = error.downcast_ref::<OverLimit>().expect("typed error");
        assert_eq!(typed.stage, OverLimitStage::Stat);
        assert_eq!(
            error.to_string(),
            "big exceeds the 4 byte verification limit"
        );
    }

    #[test]
    fn a_file_that_grows_while_reading_yields_the_typed_error() {
        let error = read_capped(&[b'x'; 8][..], 2, Path::new("grown"), 4).unwrap_err();
        let typed = error.downcast_ref::<OverLimit>().expect("typed error");
        assert_eq!(typed.stage, OverLimitStage::Read);
        assert_eq!(
            error.to_string(),
            "grown grew beyond the 4 byte verification limit while reading"
        );
    }

    #[test]
    fn read_capped_returns_a_body_at_exactly_the_limit() {
        let bytes = read_capped(&[b'x'; 4][..], 4, Path::new("ok"), 4).unwrap();
        assert_eq!(bytes.len(), 4);
    }

    #[test]
    fn rejects_leaf_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();

        assert!(read_bounded(root.path(), Path::new("link"), 1024).is_err());
    }

    #[test]
    fn rejects_intermediate_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("dir")).unwrap();

        assert!(read_bounded(root.path(), Path::new("dir/secret"), 1024).is_err());
    }

    /// A FIFO with no writer would block a plain `open()` forever; the read
    /// is refused instead, on another thread so a regression fails the test
    /// rather than hanging it.
    #[test]
    fn rejects_fifo_without_blocking() {
        let root = tempfile::tempdir().unwrap();
        nix::unistd::mkfifo(&root.path().join("pipe"), nix::sys::stat::Mode::S_IRWXU).unwrap();
        let dir = root.path().to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = read_bounded(&dir, Path::new("pipe"), 1024);
            sender
                .send(result.map_err(|error| format!("{error:#}")))
                .unwrap();
        });

        let result = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("read_bounded blocked on a FIFO");

        let message = result.unwrap_err();
        assert!(message.contains("not a regular file"), "{message}");
    }

    fn sorted_listing(root: &OwnedFd, relative: &str) -> Vec<(OsString, EntryKind)> {
        let mut entries = list_dir_no_follow(root, Path::new(relative)).unwrap();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    #[test]
    fn lists_entries_by_kind_without_dot_entries() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("dir/sub")).unwrap();
        std::fs::write(root.path().join("dir/file"), b"").unwrap();
        nix::unistd::mkfifo(&root.path().join("dir/pipe"), Mode::S_IRWXU).unwrap();
        std::os::unix::fs::symlink("/dev", root.path().join("dir/link")).unwrap();
        let root_fd = safe_open_dirfd(root.path()).unwrap();

        assert_eq!(
            sorted_listing(&root_fd, ""),
            vec![("dir".into(), EntryKind::Directory)]
        );
        assert_eq!(
            sorted_listing(&root_fd, "dir"),
            vec![
                ("file".into(), EntryKind::File),
                ("link".into(), EntryKind::Symlink),
                ("pipe".into(), EntryKind::Special),
                ("sub".into(), EntryKind::Directory),
            ]
        );
    }

    /// A directory swapped for a symlink to an outside tree, as the last or
    /// an intermediate component, is refused rather than listed.
    #[test]
    fn refuses_to_list_through_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(outside.path().join("sub")).unwrap();
        nix::unistd::mkfifo(&outside.path().join("sub/pipe"), Mode::S_IRWXU).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("dir")).unwrap();
        let root_fd = safe_open_dirfd(root.path()).unwrap();

        for relative in ["dir", "dir/sub"] {
            assert!(list_dir_no_follow(&root_fd, Path::new(relative)).is_err());
        }
    }
}
