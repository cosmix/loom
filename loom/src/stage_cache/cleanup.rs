use super::{StageCache, PARENTS};
use crate::fs::no_follow_tree::{kind, open_directory, remove_entry};
use anyhow::{Context, Result};
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag};
use nix::sys::stat::{fstatat, Mode, SFlag};
use std::os::fd::AsFd;
use std::path::Path;

/// Remove the stage's cache root with everything below it. `Ok(true)` when it
/// was removed, `Ok(false)` when it, or a parent above it, did not exist.
/// Errors when the root is a symlink or not a directory, or a parent is a
/// symlink, and never follows a symlink below the cache base.
pub fn remove_stage_cache(cache: &StageCache) -> Result<bool> {
    let root = cache.root();
    let shown = || format!("remove stage cache {}", root.display());
    let name = root
        .file_name()
        .with_context(|| format!("stage cache root {} has no name", root.display()))?;
    let Some(parent) = open_parent(cache).with_context(shown)? else {
        return Ok(false);
    };
    match fstatat(parent.as_fd(), name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Err(Errno::ENOENT) => return Ok(false),
        Err(error) => return Err(error).with_context(shown),
        Ok(status) => anyhow::ensure!(
            kind(status.st_mode) == SFlag::S_IFDIR,
            "stage cache root {} is not a directory",
            root.display()
        ),
    }
    remove_entry(parent.as_fd(), name, 0).with_context(shown)?;
    Ok(true)
}

/// Open the directory holding the root, each component below the cache base
/// without following a symlink, as `StageCache::ensure` does. `None` when a
/// component is missing.
fn open_parent(cache: &StageCache) -> Result<Option<Dir>> {
    let base = match cache.base.canonicalize() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        base => base.with_context(|| format!("resolve cache base {}", cache.base.display()))?,
    };
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC;
    let mut directory = match Dir::open(&base, flags, Mode::empty()) {
        Err(Errno::ENOENT) => return Ok(None),
        opened => opened.with_context(|| format!("open cache base {}", base.display()))?,
    };
    let mut path = base;
    for name in PARENTS {
        path.push(name);
        directory = match open_directory(directory.as_fd(), name) {
            Err(Errno::ENOENT) => return Ok(None),
            opened => opened.with_context(|| {
                format!(
                    "stage cache path {} must be a directory, not a symlink",
                    path.display()
                )
            })?,
        };
    }
    Ok(Some(directory))
}

/// Remove the cache of `stage_id` in the repository at `repo_root`, resolved
/// as the stage's launch resolves it ([`StageCache::resolve_from_process`]).
pub fn remove_stage_cache_of(repo_root: &Path, stage_id: &str) -> Result<bool> {
    remove_stage_cache(&StageCache::resolve_from_process(repo_root, stage_id)?)
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
