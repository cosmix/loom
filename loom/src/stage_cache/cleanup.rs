//! Removal of a stage's cache root when the stage's worktree goes. The root is
//! writable by the stage's own code, so the walk never follows a symlink: a
//! link planted inside is unlinked, and a root that is itself a link is
//! refused, so removal cannot reach a directory outside the root.

use super::StageCache;
use anyhow::{Context, Result};
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag};
use nix::sys::stat::{fchmod, fchmodat, fstatat, FchmodatFlags, Mode, SFlag};
use nix::unistd::{unlinkat, UnlinkatFlags};
use nix::NixPath;
use std::ffi::{OsStr, OsString};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Deeper trees are refused rather than recursed into without bound.
const MAX_DEPTH: usize = 64;

/// Remove the stage's cache root with everything below it. `Ok(true)` when it
/// was removed, `Ok(false)` when it did not exist. Errors when the root is a
/// symlink or not a directory, and never follows a symlink at or below it.
pub fn remove_stage_cache(cache: &StageCache) -> Result<bool> {
    let root = cache.root();
    let shown = || format!("remove stage cache {}", root.display());
    match root.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).with_context(shown),
        Ok(meta) => anyhow::ensure!(
            meta.is_dir(),
            "stage cache root {} is not a directory",
            root.display()
        ),
    }
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        anyhow::bail!("stage cache root {} has no parent", root.display());
    };
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC;
    let parent = match Dir::open(parent, flags, Mode::empty()) {
        Err(Errno::ENOENT) => return Ok(false),
        opened => opened.with_context(shown)?,
    };
    remove_entry(parent.as_fd(), name, 0).with_context(shown)?;
    Ok(true)
}

/// Remove the cache of `stage_id` in the repository at `repo_root`, resolved
/// from `HOME` and `XDG_CACHE_HOME` as the stage's launch resolves it.
pub fn remove_stage_cache_of(repo_root: &Path, stage_id: &str) -> Result<bool> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    let xdg = std::env::var_os("XDG_CACHE_HOME");
    let cache = StageCache::resolve(
        repo_root,
        stage_id,
        Path::new(&home),
        xdg.as_deref().map(Path::new),
    )?;
    remove_stage_cache(&cache)
}

/// Open a directory entry without following a final symlink. One closed to
/// its owner is opened to the owner first.
fn open_directory<P: ?Sized + NixPath>(parent: BorrowedFd<'_>, name: &P) -> nix::Result<Dir> {
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    match Dir::openat(parent, name, flags, Mode::empty()) {
        Err(Errno::EACCES) => {
            fchmodat(parent, name, Mode::S_IRWXU, FchmodatFlags::NoFollowSymlink)?;
            Dir::openat(parent, name, flags, Mode::empty())
        }
        result => result,
    }
}

fn entries(directory: &mut Dir) -> nix::Result<Vec<OsString>> {
    directory
        .iter()
        .filter_map(|entry| match entry {
            Ok(entry) => {
                let name = OsStr::from_bytes(entry.file_name().to_bytes());
                (name != "." && name != "..").then(|| Ok(name.to_os_string()))
            }
            Err(error) => Some(Err(error)),
        })
        .collect()
}

/// Remove `name` under `parent`, a directory with its contents, never
/// following a symlink. An entry already gone is fine.
fn remove_entry(parent: BorrowedFd<'_>, name: &OsStr, depth: usize) -> nix::Result<()> {
    let status = match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Err(Errno::ENOENT) => return Ok(()),
        status => status?,
    };
    let is_dir = SFlag::from_bits_truncate(status.st_mode) & SFlag::S_IFMT == SFlag::S_IFDIR;
    let removed = if is_dir {
        remove_tree(parent, name, depth)
    } else {
        unlinkat(parent, name, UnlinkatFlags::NoRemoveDir)
    };
    match removed {
        Err(Errno::ENOENT) => Ok(()),
        removed => removed,
    }
}

fn remove_tree(parent: BorrowedFd<'_>, name: &OsStr, depth: usize) -> nix::Result<()> {
    if depth >= MAX_DEPTH {
        return Err(Errno::ELOOP);
    }
    let mut directory = open_directory(parent, name)?;
    // A directory closed to writes (go's module cache) cannot be emptied
    // until its owner opens it.
    fchmod(&directory, Mode::S_IRWXU)?;
    for child in entries(&mut directory)? {
        remove_entry(directory.as_fd(), &child, depth + 1)?;
    }
    unlinkat(parent, name, UnlinkatFlags::RemoveDir)
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
