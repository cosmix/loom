//! The no-follow file primitives `store` builds on. Every path is relative to
//! a descriptor on the canonical state root and is opened through
//! `fs::safe_fs`, so no symlink is followed at any component; every write is a
//! temp file renamed into place in the same directory.

use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use nix::errno::Errno;
use nix::unistd::{unlinkat, UnlinkatFlags};
use serde::de::DeserializeOwned;

use crate::fs::safe_fs;
use crate::fs::safe_read::{is_not_found, list_dir_no_follow, read_bounded, EntryKind};

pub(super) const FILE_MODE: u32 = 0o600;
const DIR_MODE: libc::mode_t = 0o700;

fn canonical_root(work_dir: &Path) -> Result<PathBuf> {
    work_dir
        .canonicalize()
        .with_context(|| format!("cannot resolve the state root {}", work_dir.display()))
}

/// A descriptor on the canonical state root. `work_dir` may be a symlink (a
/// worktree's `.loom/work`); nothing beneath it may be.
pub(super) fn open_root(work_dir: &Path) -> Result<OwnedFd> {
    safe_fs::safe_open_dirfd(&canonical_root(work_dir)?)
}

/// Refuse a symlink anywhere on `rel`, or a final entry that is not a regular
/// file. An absent path (or absent parent) is fine.
pub(super) fn refuse_unsafe(root: &OwnedFd, rel: &Path) -> Result<()> {
    let flags = libc::O_RDONLY | libc::O_NONBLOCK;
    match safe_fs::open_safely(root.as_raw_fd(), rel, flags, 0) {
        Ok(fd) => {
            let is_file = File::from(fd).metadata()?.is_file();
            ensure!(is_file, "{} is not a regular file", rel.display());
            Ok(())
        }
        Err(error) if is_not_found(&error) => Ok(()),
        Err(error) => Err(error.context(format!("refusing unsafe path {}", rel.display()))),
    }
}

/// Create every directory of `rel`, refusing a symlinked component.
pub(super) fn create_dirs(root: &OwnedFd, rel: &Path) -> Result<()> {
    safe_fs::safe_create_dir_all_in_workdir(root.as_raw_fd(), rel, DIR_MODE)
        .with_context(|| format!("cannot create {}", rel.display()))
}

/// Replace `rel` with `bytes` through a temp file in the same directory. The
/// directory must exist: a job directory already pruned is not recreated.
pub(super) fn write_atomic(root: &OwnedFd, rel: &Path, bytes: &[u8]) -> Result<()> {
    refuse_unsafe(root, rel)?;
    let name = rel
        .file_name()
        .and_then(|name| name.to_str())
        .context("a job file path names no file")?;
    let temp = rel.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let written = write_new(root, &temp, bytes)
        .and_then(|()| safe_fs::safe_rename_in_workdir(root.as_raw_fd(), &temp, rel));
    if written.is_err() {
        let _ = safe_fs::safe_remove_in_workdir(root.as_raw_fd(), &temp);
    }
    written.with_context(|| format!("cannot write {}", rel.display()))
}

fn write_new(root: &OwnedFd, rel: &Path, bytes: &[u8]) -> Result<()> {
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL;
    let mut file = File::from(safe_fs::open_safely(
        root.as_raw_fd(),
        rel,
        flags,
        FILE_MODE,
    )?);
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Append to `rel` under an flock of the file itself. `choose` sees the file
/// and its length and returns what to write, if anything.
pub(super) fn append_locked(
    root: &OwnedFd,
    rel: &Path,
    choose: impl FnOnce(&File, u64) -> Result<Option<Vec<u8>>>,
) -> Result<()> {
    let flags = libc::O_RDWR | libc::O_APPEND | libc::O_CREAT;
    let fd = safe_fs::open_safely(root.as_raw_fd(), rel, flags, FILE_MODE)
        .with_context(|| format!("cannot open {}", rel.display()))?;
    safe_fs::flock_exclusive(&fd)?;
    let mut file = File::from(fd);
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "{} is not a regular file",
        rel.display()
    );
    if let Some(bytes) = choose(&file, metadata.len())? {
        file.write_all(&bytes)
            .with_context(|| format!("cannot append to {}", rel.display()))?;
    }
    Ok(())
}

/// Whether the `len` bytes of `file` end with `tail`.
pub(super) fn ends_with(file: &File, len: u64, tail: &[u8]) -> Result<bool> {
    let Some(start) = len.checked_sub(tail.len() as u64) else {
        return Ok(false);
    };
    let mut last = vec![0; tail.len()];
    file.read_exact_at(&mut last, start)?;
    Ok(last == tail)
}

/// Read at most `max` bytes of `rel` beneath the state root; an absent file,
/// directory or state root is `None`.
pub(super) fn read_in(work_dir: &Path, rel: &Path, max: usize) -> Result<Option<Vec<u8>>> {
    match canonical_root(work_dir).and_then(|root| read_bounded(&root, rel, max)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn read_json<T: DeserializeOwned>(
    work_dir: &Path,
    rel: &Path,
    max: usize,
) -> Result<Option<T>> {
    let Some(bytes) = read_in(work_dir, rel, max)? else {
        return Ok(None);
    };
    let value =
        serde_json::from_slice(&bytes).with_context(|| format!("malformed {}", rel.display()))?;
    Ok(Some(value))
}

/// Remove the job directory `name` under `stage` and the files in it,
/// following no symlink. A job directory holds files only; a nested
/// directory is refused.
pub(super) fn remove_job_dir(stage: &OwnedFd, name: &str) -> Result<()> {
    let flags = libc::O_DIRECTORY | libc::O_RDONLY;
    let job = safe_fs::open_safely(stage.as_raw_fd(), Path::new(name), flags, 0)?;
    for (entry, kind) in list_dir_no_follow(&job, Path::new(""))? {
        ensure!(
            kind != EntryKind::Directory,
            "unexpected directory {entry:?} in a job directory"
        );
        match unlinkat(&job, entry.as_os_str(), UnlinkatFlags::NoRemoveDir) {
            Ok(()) | Err(Errno::ENOENT) => {}
            Err(error) => return Err(error.into()),
        }
    }
    match unlinkat(stage, name, UnlinkatFlags::RemoveDir) {
        Ok(()) | Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
