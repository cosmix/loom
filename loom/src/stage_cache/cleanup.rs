use super::base_checkout::BASE_CHECKOUT_PARENTS;
use super::{StageCache, PARENTS};
use crate::fs::no_follow_tree::{kind, open_directory, remove_entry};
use anyhow::{Context, Result};
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag};
use nix::sys::stat::{fstatat, Mode, SFlag};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

/// Remove the stage's cache root with everything below it, and the stage's
/// base checkouts kept beside it (`base_checkout`). `Ok(true)` when the root
/// was removed, `Ok(false)` when it, or a parent above it, did not exist.
/// Errors when the root is a symlink or not a directory, or a parent is a
/// symlink, and never follows a symlink below the cache base. Both removals
/// run even when the first fails; the first error is the one returned.
pub fn remove_stage_cache(cache: &StageCache) -> Result<bool> {
    let removed = remove_below(cache, &PARENTS, "stage cache");
    let checkouts = remove_below(cache, &BASE_CHECKOUT_PARENTS, "stage base checkouts");
    let removed = removed?;
    checkouts?;
    Ok(removed)
}

/// Remove `<base>/<parents..>/<root name>`, the `what` of the stage.
fn remove_below(cache: &StageCache, parents: &[&str], what: &str) -> Result<bool> {
    let name = cache.root_name()?;
    let Some((parent, path)) =
        open_parent(cache, parents).with_context(|| format!("remove {what}"))?
    else {
        return Ok(false);
    };
    let path = path.join(name);
    let shown = || format!("remove {what} {}", path.display());
    match fstatat(parent.as_fd(), name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Err(Errno::ENOENT) => return Ok(false),
        Err(error) => return Err(error).with_context(shown),
        Ok(status) => anyhow::ensure!(
            kind(status.st_mode) == SFlag::S_IFDIR,
            "{what} {} is not a directory",
            path.display()
        ),
    }
    remove_entry(parent.as_fd(), name, 0).with_context(shown)?;
    Ok(true)
}

/// Open `<base>/<parents..>`, each component below the cache base without
/// following a symlink, as `StageCache::ensure` does, with its path. `None`
/// when a component is missing.
fn open_parent(cache: &StageCache, parents: &[&str]) -> Result<Option<(Dir, PathBuf)>> {
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
    for name in parents {
        path.push(name);
        directory = match open_directory(directory.as_fd(), *name) {
            Err(Errno::ENOENT) => return Ok(None),
            opened => opened.with_context(|| {
                format!(
                    "stage cache path {} must be a directory, not a symlink",
                    path.display()
                )
            })?,
        };
    }
    Ok(Some((directory, path)))
}

/// Remove the cache of `stage_id` in the repository at `repo_root`, resolved
/// as the stage's launch resolves it ([`StageCache::resolve_from_process`]).
pub fn remove_stage_cache_of(repo_root: &Path, stage_id: &str) -> Result<bool> {
    remove_stage_cache(&StageCache::resolve_from_process(repo_root, stage_id)?)
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
