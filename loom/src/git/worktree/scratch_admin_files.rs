//! File operations on a worktree's administrative directory, a directory a
//! session can write: everything goes through a descriptor opened without
//! following a link, no component is followed, and a file is replaced by
//! renaming its lock file over it, so a link planted at the name is replaced
//! instead of written through.

use anyhow::{ensure, Context, Result};
use nix::errno::Errno;
use nix::fcntl::{renameat, AtFlags};
use nix::sys::stat::{fstatat, mkdirat, Mode};
use nix::unistd::{unlinkat, UnlinkatFlags};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

use crate::fs::safe_fs::open_safely;
use crate::fs::safe_read::{is_not_found, single_link_regular_file};

/// Directory under the state root that holds the private directories.
pub(super) const PRIVATE_PARENT: &str = "pinned-git";
/// Older than this is a directory a killed process left behind: no git command
/// loom runs outlives its deadline by anything close to it.
const LEFTOVER_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// More than any index or pseudoref git writes; a larger file is a session's
/// way of filling the host's disk through the copy.
const MAX_COPIED_BYTES: u64 = 1 << 30;

/// What one command's copies may add up to. Every copied name but the split-index
/// files is a single file, and a re-split takes 20% of the index's entries changing
/// (`splitIndex.maxPercentChange`), so a worktree holds a few of those. A session
/// can write as many as it likes; revisit if a real worktree reaches these.
const MAX_COPIED_TOTAL_BYTES: u64 = 4 << 30;
const MAX_COPIED_FILES: usize = 256;

/// Enough of a file's identity to tell it was replaced or rewritten. Git
/// replaces the index and refs by rename and rewrites the message files in
/// place, and a copy keeps its source's modification time, so either write
/// changes this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stamp {
    inode: u64,
    len: u64,
    mtime: i64,
    mtime_nsec: i64,
}

impl Stamp {
    fn of(meta: &std::fs::Metadata) -> Self {
        Self {
            inode: meta.ino(),
            len: meta.len(),
            mtime: meta.mtime(),
            mtime_nsec: meta.mtime_nsec(),
        }
    }

    /// The stamp of the private file at `path`, `None` when it is gone.
    pub(super) fn of_private(path: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Ok(meta) => Ok(Some(Self::of(&meta))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("cannot inspect {}", path.display())),
        }
    }
}

/// A refusal to run git over what a session left in its directory. Naming the
/// path keeps it actionable for whoever reads a failed stage.
fn refused(admin_path: &Path, name: &OsStr, why: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "loom will not run git in the worktree: {} {why}; git never makes one there",
        admin_path.join(name).display()
    )
}

/// The regular file `relative` under `admin`, opened without following any
/// link; `None` when absent. A link, a FIFO or a device there is refused.
pub(in crate::git::worktree) fn open_regular(
    admin: &OwnedFd,
    admin_path: &Path,
    relative: &str,
) -> Result<Option<File>> {
    let flags = libc::O_RDONLY | libc::O_NONBLOCK;
    let opened = open_safely(admin.as_raw_fd(), Path::new(relative), flags, 0);
    let file = match opened {
        Ok(descriptor) => File::from(descriptor),
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => {
            let why = format!("is not a regular file ({error:#})");
            return Err(refused(admin_path, OsStr::new(relative), &why));
        }
    };
    let regular = file.metadata().is_ok_and(|meta| meta.is_file());
    if !regular {
        return Err(refused(
            admin_path,
            OsStr::new(relative),
            "is not a regular file",
        ));
    }
    Ok(Some(file))
}

/// The bytes `names` may take in all, refusing up front, on sizes alone, when they
/// are too many or already too large: a refused command then copied nothing. The
/// copies themselves spend the result, which bounds a file that grows afterwards.
pub(super) fn copy_budget(admin: &OwnedFd, admin_path: &Path, names: &[OsString]) -> Result<u64> {
    ensure!(
        names.len() <= MAX_COPIED_FILES,
        "loom will not run git in the worktree: {} holds {} files for git, over the limit of \
         {MAX_COPIED_FILES}; remove the sharedindex.* files the index does not name",
        admin_path.display(),
        names.len()
    );
    let mut total = 0u64;
    for name in names {
        let size = stamp_at(admin, name)?.map_or(0, |stamp| stamp.len);
        total = total.saturating_add(size);
    }
    ensure!(
        total <= MAX_COPIED_TOTAL_BYTES,
        "loom will not run git in the worktree: the files for git in {} hold {total} bytes, over \
         the limit of {MAX_COPIED_TOTAL_BYTES}; remove the sharedindex.* files the index does \
         not name",
        admin_path.display()
    );
    Ok(MAX_COPIED_TOTAL_BYTES)
}

/// Copy `name` from `admin` to `into`, keeping its mode and modification time
/// (git compares file times with the index's own to spot racy entries), and
/// spending what it wrote from `budget`.
/// Returns the real file's stamp and the copy's; `None` when absent.
pub(super) fn copy_in(
    admin: &OwnedFd,
    admin_path: &Path,
    name: &OsStr,
    into: &Path,
    budget: &mut u64,
) -> Result<Option<(Stamp, Stamp)>> {
    let text = name
        .to_str()
        .context("a copied entry's name is not UTF-8")?;
    let Some(source) = open_regular(admin, admin_path, text)? else {
        return Ok(None);
    };
    let meta = source.metadata().context("cannot inspect an opened file")?;
    ensure!(
        meta.len() <= MAX_COPIED_BYTES,
        "loom will not run git in the worktree: {} is over {MAX_COPIED_BYTES} bytes",
        admin_path.join(name).display()
    );
    ensure!(
        meta.len() <= *budget,
        "loom will not run git in the worktree: {} grew past the {MAX_COPIED_TOTAL_BYTES} bytes \
         one command may copy",
        admin_path.join(name).display()
    );
    let target = into.join(name);
    let mut copy = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(meta.mode() & 0o777)
        .open(&target)
        .with_context(|| format!("cannot create {}", target.display()))?;
    let limit = MAX_COPIED_BYTES.min(*budget);
    let copied = std::io::copy(&mut source.take(limit), &mut copy)
        .with_context(|| format!("cannot copy {}", admin_path.join(name).display()))?;
    *budget -= copied;
    copy.set_modified(meta.modified()?)?;
    let private = Stamp::of(&copy.metadata()?);
    Ok(Some((Stamp::of(&meta), private)))
}

/// The stamp of `name` in `admin` without following a link; `None` when absent.
fn stamp_at(admin: &OwnedFd, name: &OsStr) -> Result<Option<Stamp>> {
    match fstatat(admin, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(Some(Stamp {
            inode: stat.st_ino,
            len: stat.st_size as u64,
            mtime: stat.st_mtime,
            mtime_nsec: stat.st_mtime_nsec,
        })),
        Err(Errno::ENOENT) => Ok(None),
        Err(error) => Err(error).context("cannot inspect an administrative file"),
    }
}

/// Whether `admin` has an entry `name`, a link counting as one.
pub(super) fn is_present(admin: &OwnedFd, name: &OsStr) -> bool {
    stamp_at(admin, name).is_ok_and(|stamp| stamp.is_some())
}

/// `name.lock` in `admin`, git's own lock for the name, held until dropped.
struct Lock<'a> {
    admin: &'a OwnedFd,
    name: OsString,
    file: File,
    held: bool,
}

impl<'a> Lock<'a> {
    /// Take the lock as git takes it: `O_CREAT | O_EXCL`, so a concurrent git
    /// holding it stops this and this stops a concurrent git.
    fn take(admin: &'a OwnedFd, admin_path: &Path, name: &OsStr, mode: u32) -> Result<Self> {
        let mut bytes = name.to_os_string().into_vec();
        bytes.extend_from_slice(b".lock");
        let lock = OsString::from_vec(bytes);
        let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL;
        let descriptor = open_safely(admin.as_raw_fd(), Path::new(&lock), flags, mode)
            .with_context(|| {
                format!(
                    "cannot lock {}: another git holds it, or it was left behind",
                    admin_path.join(name).display()
                )
            })?;
        Ok(Self {
            admin,
            name: lock,
            file: File::from(descriptor),
            held: true,
        })
    }

    /// Replace `target` with what the lock file holds, releasing the lock.
    fn commit(mut self, target: &OsStr) -> Result<()> {
        renameat(self.admin, self.name.as_os_str(), self.admin, target)
            .context("cannot rename a lock file over its target")?;
        self.held = false;
        Ok(())
    }
}

impl Drop for Lock<'_> {
    fn drop(&mut self) {
        if self.held {
            let _ = unlinkat(
                self.admin,
                self.name.as_os_str(),
                UnlinkatFlags::NoRemoveDir,
            );
        }
    }
}

/// Compare and swap one file: under `name.lock`, confirm `name` is still what
/// `expected` recorded, then replace it with `source` (or remove it, with
/// `source` `None`). Nothing that honours the lock can change it in between.
pub(super) fn swap(
    admin: &OwnedFd,
    admin_path: &Path,
    name: &OsStr,
    expected: Option<Stamp>,
    source: Option<&Path>,
) -> Result<()> {
    let mode = match source {
        Some(path) => std::fs::metadata(path)?.permissions().mode() & 0o777,
        None => 0o600,
    };
    let mut lock = Lock::take(admin, admin_path, name, mode)?;
    ensure!(
        stamp_at(admin, name)? == expected,
        "{} changed while git was updating it; the update was not published",
        admin_path.join(name).display()
    );
    let Some(source) = source else {
        let removed = unlinkat(admin, name, UnlinkatFlags::NoRemoveDir);
        drop(lock);
        return match removed {
            Ok(()) | Err(Errno::ENOENT) => Ok(()),
            Err(error) => Err(error).context("cannot remove an administrative file"),
        };
    };
    let bytes =
        std::fs::read(source).with_context(|| format!("cannot read {}", source.display()))?;
    lock.file
        .write_all(&bytes)
        .with_context(|| format!("cannot publish {}", admin_path.join(name).display()))?;
    lock.file
        .set_modified(std::fs::metadata(source)?.modified()?)?;
    lock.commit(name)
}

/// Append `entries` to `logs/HEAD` in `admin`. The open follows no link at any
/// component and refuses a file with a second name, the one other way a
/// session could aim the append at a file it cannot write.
pub(super) fn append_head_reflog(admin: &OwnedFd, entries: &[u8]) -> Result<()> {
    match mkdirat(admin, "logs", Mode::from_bits_truncate(0o777)) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(error).context("cannot create logs"),
    }
    let flags = libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT | libc::O_NONBLOCK;
    let descriptor = open_safely(admin.as_raw_fd(), Path::new("logs/HEAD"), flags, 0o666)?;
    let mut file = File::from(descriptor);
    single_link_regular_file(&file)?;
    file.write_all(entries)
        .context("cannot append to logs/HEAD")
}

/// Refuse a planted `logs` or `logs/HEAD`. Git never opens these in a private
/// directory, but the reflog entry is appended to them afterwards, and a link
/// there means the session tried to aim it elsewhere. Returns whether the file
/// exists.
pub(super) fn refuse_planted_reflog(admin: &OwnedFd, admin_path: &Path) -> Result<bool> {
    let Some(file) = open_regular(admin, admin_path, "logs/HEAD")? else {
        return Ok(false);
    };
    single_link_regular_file(&file)
        .map_err(|_| refused(admin_path, OsStr::new("logs/HEAD"), "has a second name"))?;
    Ok(true)
}

/// Neither the state root nor the system's temporary directory can be written:
/// only a process inside a sandbox, running with its session's own privileges,
/// is that confined.
#[derive(Debug)]
pub(in crate::git::worktree) struct NoPrivateDirectory(std::io::Error);

impl std::fmt::Display for NoPrivateDirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot create a private git directory: {}", self.0)
    }
}

impl std::error::Error for NoPrivateDirectory {}

/// A new private directory under the state root of `repo_root`. A process
/// that cannot write there is a sandboxed CLI, already confined to what its
/// session controls, and a repository with no state root has no sessions;
/// both use the system's temporary directory.
pub(super) fn private_dir(repo_root: &Path) -> Result<TempDir> {
    if let Some(state) = super::super::main_identity::state_dir_of(repo_root)? {
        let parent = state.join(PRIVATE_PARENT);
        reap_leftovers(&parent);
        let created =
            std::fs::create_dir_all(&parent).and_then(|()| owner_only().tempdir_in(&parent));
        match created {
            Ok(dir) => return Ok(dir),
            Err(error) if is_confinement_denial(&error) => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot create a directory in {}", parent.display()))
            }
        }
    }
    match owner_only().tempdir() {
        Ok(dir) => Ok(dir),
        Err(error) if is_confinement_denial(&error) => Err(NoPrivateDirectory(error).into()),
        Err(error) => Err(error).context("cannot create a private git directory"),
    }
}

/// The temporary-directory builder's own default mode follows the umask.
fn owner_only() -> tempfile::Builder<'static, 'static> {
    let mut builder = tempfile::Builder::new();
    builder.permissions(std::fs::Permissions::from_mode(0o700));
    builder
}

fn is_confinement_denial(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    )
}

/// Remove what a killed process left under `parent`. Sessions cannot write
/// there, so nothing in it is a link they planted, and `remove_dir_all` follows
/// no link anyway.
fn reap_leftovers(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let leftover = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > LEFTOVER_AGE));
        if leftover {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
