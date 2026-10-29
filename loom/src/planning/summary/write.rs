//! Reads and locked writes of summary files under the store.
//!
//! Every read is no-follow and bounded; every write runs inside [`with_store_lock`] through
//! `safe_fs`, so a symlink planted anywhere under `doc/loom/summaries/` fails the operation.
//! A write goes to `.<name>.tmp-<pid>` beside the target and is renamed over it; the leading
//! dot keeps the watcher and discovery from ever reporting the temp file.

#[cfg(test)]
mod tests;

use std::ffi::OsStr;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;

use anyhow::{anyhow, Result};

use super::lock::with_store_lock;
use super::store::{self, STORE_DIR};
use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_create_new_in_workdir, safe_open_dirfd,
    safe_remove_in_workdir, safe_rename_in_workdir,
};
use crate::fs::safe_read::{is_not_found, list_dir_no_follow, read_to_string_bounded, EntryKind};
use crate::planning::discover::MAX_DOC_BYTES;
use crate::planning::runtime::RuntimePaths;
use crate::planning::sections::normalized_hash;
use crate::planning::watch::OwnWrites;

/// Whether [`write_summary_if`] wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The summary file was written.
    Written,
    /// The file on disk no longer had the expected hash; nothing was written.
    Changed,
}

/// The text of summary `id`, read no-follow; `None` when the file is absent. A symlink, a
/// non-regular file, invalid UTF-8 or a file over the document size cap is an error.
pub fn read_summary(root: &Path, id: &str) -> Result<Option<String>> {
    store::validate_id(id)?;
    match read_to_string_bounded(root, &store::summary_path(id), MAX_DOC_BYTES) {
        Ok(text) => Ok(Some(text)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Ids of the summary files present in the store (`<id>.md` for every valid id), sorted.
/// Hidden entries (temp files), symlinked directories and names that are not valid ids are
/// skipped. An absent store has no ids.
pub(crate) fn stored_ids(root: &Path) -> Result<Vec<String>> {
    let dirfd = safe_open_dirfd(root)?;
    let mut ids = Vec::new();
    let mut pending = vec![String::new()];
    while let Some(prefix) = pending.pop() {
        let dir = Path::new(STORE_DIR).join(&prefix);
        for (name, kind) in listing(&dirfd, &dir)? {
            if name.starts_with('.') {
                continue;
            }
            if kind == EntryKind::Directory {
                // Valid ids have at most three components. Bound traversal even when
                // unrelated directories are planted in the store.
                if prefix.bytes().filter(|b| *b == b'/').count() < 2 {
                    pending.push(format!("{prefix}{name}/"));
                }
            } else {
                ids.extend(id_of(&prefix, &name));
            }
        }
    }
    ids.sort();
    Ok(ids)
}

/// The entries of `dir` under `dirfd` as UTF-8 names; an absent directory is empty.
fn listing(dirfd: &OwnedFd, dir: &Path) -> Result<Vec<(String, EntryKind)>> {
    let entries = match list_dir_no_follow(dirfd, dir) {
        Ok(entries) => entries,
        Err(error) if is_not_found(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let named = entries
        .into_iter()
        .filter_map(|(name, kind)| Some((name.into_string().ok()?, kind)));
    Ok(named.collect())
}

/// The id of store file `name` in the slash-terminated directory `prefix`.
fn id_of(prefix: &str, name: &str) -> Option<String> {
    let stem = name
        .strip_suffix(".md")
        .filter(|_| !name.starts_with('.'))?;
    let id = format!("{prefix}{stem}");
    store::validate_id(&id).is_ok().then_some(id)
}

/// Write `text` as summary `id` under the store lock, unconditionally, registering it with
/// `own` before the rename that makes it visible.
pub fn write_summary(
    root: &Path,
    paths: &RuntimePaths,
    id: &str,
    text: &str,
    own: Option<&OwnWrites>,
) -> Result<()> {
    store::validate_id(id)?;
    with_store_lock(paths, || write_locked(root, id, text, own))
}

/// Read-validate-write under one hold of the store lock: re-read summary `id`, and only when
/// its `normalized_hash` equals `expected_file_hash` (`None`: the file must be absent) call
/// `build` and write its text. Any other content returns [`WriteOutcome::Changed`] unwritten.
/// `build` runs under the lock, so it must not take the store lock itself.
pub fn write_summary_if(
    root: &Path,
    paths: &RuntimePaths,
    id: &str,
    expected_file_hash: Option<&str>,
    build: impl FnOnce() -> Result<String>,
    own: Option<&OwnWrites>,
) -> Result<WriteOutcome> {
    store::validate_id(id)?;
    with_store_lock(paths, || {
        let current = read_summary(root, id)?.map(|text| normalized_hash(&text));
        if current.as_deref() != expected_file_hash {
            return Ok(WriteOutcome::Changed);
        }
        let text = build()?;
        write_locked(root, id, &text, own)?;
        Ok(WriteOutcome::Written)
    })
}

/// Delete summary `id` under the store lock; an absent file is not an error.
pub fn delete_summary(root: &Path, paths: &RuntimePaths, id: &str) -> Result<()> {
    store::validate_id(id)?;
    with_store_lock(paths, || {
        let dirfd = safe_open_dirfd(root)?;
        safe_remove_in_workdir(dirfd.as_raw_fd(), &store::summary_path(id))
    })
}

/// Write `text` to summary `id` by an exclusive temp file and a rename; the caller holds the
/// store lock. `own` gets the path and hash registered BEFORE the rename (the watcher may walk
/// between the rename and a later registration) and withdrawn when the rename fails.
fn write_locked(root: &Path, id: &str, text: &str, own: Option<&OwnWrites>) -> Result<()> {
    let target = store::summary_path(id);
    let (dir, name) = split_target(&target)?;
    let dirfd = safe_open_dirfd(root)?;
    let fd = dirfd.as_raw_fd();
    safe_create_dir_all_in_workdir(fd, dir, 0o755)?;
    remove_stale_temps(&dirfd, dir, name)?;
    let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    safe_create_new_in_workdir(fd, &temp, text.as_bytes())?;
    if let Some(own) = own {
        own.register(&target, &normalized_hash(text));
    }
    if let Err(error) = safe_rename_in_workdir(fd, &temp, &target) {
        if let Some(own) = own {
            own.unregister(&target);
        }
        let _ = safe_remove_in_workdir(fd, &temp);
        return Err(error);
    }
    Ok(())
}

/// The directory and file name of a store path.
fn split_target(target: &Path) -> Result<(&Path, &str)> {
    let dir = target.parent();
    let name = target.file_name().and_then(OsStr::to_str);
    dir.zip(name)
        .ok_or_else(|| anyhow!("summary path {} has no file name", target.display()))
}

/// Remove every `.<name>.tmp-*` in `dir`, whatever its pid: under the store lock no other
/// writer is mid-write, so each one is the leftover of a crashed write.
fn remove_stale_temps(dirfd: &OwnedFd, dir: &Path, name: &str) -> Result<()> {
    let prefix = format!(".{name}.tmp-");
    for (entry, _) in listing(dirfd, dir)? {
        if entry.starts_with(&prefix) {
            safe_remove_in_workdir(dirfd.as_raw_fd(), &dir.join(&entry))?;
        }
    }
    Ok(())
}

/// Repository-relative path of summary `id`'s temp file for this process (tests only).
#[cfg(test)]
pub(crate) fn temp_path(id: &str, pid: u32) -> std::path::PathBuf {
    let target = store::summary_path(id);
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.tmp-{pid}"))
}
