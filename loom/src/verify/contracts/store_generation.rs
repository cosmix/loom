//! Immutable copies for version 2 freeze records.
//!
//! A generation is staged whole under `staging/`, every hash checked and every
//! file and directory synced, then renamed to `generations/<digest>/`, where
//! `<digest>` names its file/hash set. The record that names the set is
//! published after that, so a reader holding either record sees all of one
//! generation. Only `staging/` is collected, and only after publication; a
//! committed generation stays for readers of records that name it.

use anyhow::{bail, Context, Result};
use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use super::{
    FreezeRecord, FrozenFile, FREEZE_RECORD_VERSION, GENERATION_RECORD_VERSION,
    MAX_FROZEN_FILE_BYTES,
};
use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_locked_write_in_workdir, safe_open_dirfd,
    safe_rename_in_workdir,
};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::fs::safe_replace::publish_in_dir;
use crate::relay::sha256_hex;

const GENERATIONS: &str = "generations";
const READY: &[u8] = b"complete\n";
#[path = "store_generation_gc.rs"]
mod gc;

pub(super) fn cleanup(dir: &Path) -> Result<()> {
    gc::cleanup(dir)
}

pub(super) fn write_initial(
    dir: &Path,
    record: &FreezeRecord,
    files: &[(String, Vec<u8>)],
) -> Result<()> {
    if record.version == GENERATION_RECORD_VERSION {
        return stage(dir, record, files);
    }
    if record.version != FREEZE_RECORD_VERSION {
        bail!("unsupported freeze record version {}", record.version);
    }
    // `freeze.json` ends the contract phase, so its copies must be durable first.
    let dirfd = safe_open_dirfd(dir)?;
    let mut dirs = vec![PathBuf::new()];
    copy_staged(&dirfd, Path::new("files"), files, &mut dirs)?;
    sync_dirs(&dirfd, &mut dirs)
}

pub(super) fn files_root(dir: &Path, record: &FreezeRecord) -> PathBuf {
    dir.join(generation(&record.files)).join("files")
}

fn generation(files: &[FrozenFile]) -> PathBuf {
    let mut identity = Vec::new();
    for file in files {
        identity.extend_from_slice(&(file.path.len() as u64).to_be_bytes());
        identity.extend_from_slice(file.path.as_bytes());
        identity.extend_from_slice(&(file.sha256.len() as u64).to_be_bytes());
        identity.extend_from_slice(file.sha256.as_bytes());
    }
    Path::new(GENERATIONS).join(sha256_hex(&identity))
}

pub(super) fn stage(dir: &Path, record: &FreezeRecord, files: &[(String, Vec<u8>)]) -> Result<()> {
    let committed = generation(&record.files);
    let ready = committed.join("ready");
    match read_bounded(dir, &ready, READY.len()) {
        Ok(bytes) => {
            if bytes != READY {
                bail!("invalid frozen generation marker");
            }
            verify(dir, &committed, record)?;
            let dirfd = safe_open_dirfd(dir)?;
            sync_dir(&dirfd, Path::new(GENERATIONS))?;
            return Ok(());
        }
        Err(error) if is_not_found(&error) => {}
        Err(error) => return Err(error),
    }
    let dirfd = safe_open_dirfd(dir)?;
    let base = Path::new("staging").join(committed.file_name().context("generation name missing")?);
    let mut dirs = vec![
        PathBuf::new(),
        PathBuf::from(GENERATIONS),
        PathBuf::from("staging"),
        base.clone(),
        base.join("files"),
    ];
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new(GENERATIONS), 0o755)?;
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), &base.join("files"), 0o755)?;
    copy_staged(&dirfd, &base.join("files"), files, &mut dirs)?;
    verify(dir, &base, record)?;
    sync_dirs(&dirfd, &mut dirs)?;
    let parent = open_dir(&dirfd, &base)?;
    publish_in_dir(&parent, std::ffi::OsStr::new("ready"), READY, 0o644, false)?;
    sync_dir(&dirfd, &base)?;
    #[cfg(test)]
    faults::hit(faults::Point::BeforeGenerationRename)?;
    safe_rename_in_workdir(dirfd.as_raw_fd(), &base, &committed)?;
    sync_dir(&dirfd, Path::new("staging"))?;
    sync_dir(&dirfd, Path::new(GENERATIONS))?;
    Ok(())
}

/// Write each file beneath `root`, adding the directories it needed to `dirs`.
fn copy_staged(
    dirfd: &OwnedFd,
    root: &Path,
    files: &[(String, Vec<u8>)],
    dirs: &mut Vec<PathBuf>,
) -> Result<()> {
    for (path, bytes) in files {
        let target = root.join(path);
        let parent = target.parent().context("frozen file has no parent")?;
        safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), parent, 0o755)?;
        let mut ancestor = parent;
        while !ancestor.as_os_str().is_empty() {
            dirs.push(ancestor.to_path_buf());
            ancestor = ancestor.parent().unwrap_or(Path::new(""));
        }
        safe_locked_write_in_workdir(dirfd.as_raw_fd(), &target, bytes)
            .with_context(|| format!("failed to copy frozen file '{path}'"))?;
        #[cfg(test)]
        faults::hit(faults::Point::AfterCopy)?;
    }
    Ok(())
}

fn verify(dir: &Path, generation: &Path, record: &FreezeRecord) -> Result<()> {
    let base = generation.join("files");
    for file in &record.files {
        let bytes = read_bounded(dir, &base.join(&file.path), MAX_FROZEN_FILE_BYTES)?;
        if sha256_hex(&bytes) != file.sha256 {
            bail!("frozen generation hash mismatch for '{}'", file.path);
        }
    }
    Ok(())
}

fn sync_dirs(root: &OwnedFd, dirs: &mut Vec<PathBuf>) -> Result<()> {
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    dirs.dedup();
    for path in dirs {
        sync_dir(root, path)?;
    }
    Ok(())
}

fn sync_dir(root: &OwnedFd, path: &Path) -> Result<()> {
    let current = open_dir(root, path)?;
    // SAFETY: current is an open directory descriptor.
    if unsafe { libc::fsync(current.as_raw_fd()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("sync generation directory");
    }
    #[cfg(test)]
    faults::note_synced(path);
    Ok(())
}

fn open_dir(root: &OwnedFd, path: &Path) -> Result<OwnedFd> {
    let mut current = root.try_clone()?;
    for part in path.components() {
        let Component::Normal(name) = part else {
            bail!("invalid generation directory");
        };
        let name = CString::new(name.as_bytes())?;
        // SAFETY: current is open and name is a NUL-terminated component.
        let fd = unsafe {
            libc::openat(
                current.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).context("open generation directory");
        }
        // SAFETY: openat returned an owned descriptor.
        current = unsafe { OwnedFd::from_raw_fd(fd) };
    }
    Ok(current)
}

/// Injected failures for the replacement transaction's tests.
#[cfg(test)]
pub(crate) mod faults {
    use std::cell::{Cell, RefCell};
    use std::path::{Path, PathBuf};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Point {
        /// After each staged copy; arming stops at the first.
        AfterCopy,
        /// The generation is complete and marked ready, but not yet renamed.
        BeforeGenerationRename,
        /// The generation is committed; the record still names the old one.
        BeforeRecord,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Action {
        Fail,
        /// Stop the process without unwinding, as a crash would.
        Exit,
    }

    thread_local! {
        static ARMED: Cell<Option<(Point, Action)>> = const { Cell::new(None) };
        static SYNCED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn note_synced(path: &Path) {
        SYNCED.with(|synced| synced.borrow_mut().push(path.to_path_buf()));
    }

    /// The directories this thread synced since the last call.
    pub(crate) fn take_synced() -> Vec<PathBuf> {
        SYNCED.with(RefCell::take)
    }

    pub(crate) fn arm(point: Point, action: Action) {
        ARMED.with(|armed| armed.set(Some((point, action))));
    }

    pub(crate) fn hit(point: Point) -> anyhow::Result<()> {
        match ARMED.with(Cell::get) {
            Some((armed, action)) if armed == point => {
                ARMED.with(|armed| armed.set(None));
                if action == Action::Exit {
                    // SAFETY: _exit has no preconditions and ends this process.
                    unsafe { libc::_exit(81) };
                }
                anyhow::bail!("injected failure at {point:?}")
            }
            _ => Ok(()),
        }
    }
}
