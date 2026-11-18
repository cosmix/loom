//! Directory listings that record the planning documents already on disk.

use std::ffi::OsString;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_read::{is_not_found, list_dir_no_follow, EntryKind};
use crate::planning::kinds::is_feature_name;
use crate::planning::slug::plan_home;

pub(super) fn entries(fd: &OwnedFd, dir: &Path) -> Result<Vec<(OsString, EntryKind)>> {
    match list_dir_no_follow(fd, dir) {
        Ok(entries) => Ok(entries),
        Err(error) if is_not_found(&error) => Ok(Vec::new()),
        Err(error) => Err(error.context(format!("cannot list {}", dir.display()))),
    }
}

pub(super) fn snapshot(root: &Path) -> Result<Vec<PathBuf>> {
    let fd = safe_open_dirfd(root)?;
    let mut existing = Vec::new();
    for (name, kind) in entries(&fd, Path::new(""))? {
        if name != "doc" {
            continue;
        }
        if kind != EntryKind::Directory {
            existing.push(name.into());
            continue;
        }
        for (name, kind) in entries(&fd, Path::new("doc"))? {
            let path = Path::new("doc").join(&name);
            if name != "product" && name != "plans" {
                continue;
            }
            if kind != EntryKind::Directory {
                existing.push(path);
            } else if name == "product" {
                existing.extend(
                    entries(&fd, &path)?
                        .into_iter()
                        .map(|(name, _)| path.join(name)),
                );
            } else {
                snapshot_plans(&fd, &path, &mut existing)?;
            }
        }
    }
    existing.sort();
    existing.dedup();
    Ok(existing)
}

fn snapshot_plans(fd: &OwnedFd, plans: &Path, existing: &mut Vec<PathBuf>) -> Result<()> {
    let mut pending = vec![plans.to_path_buf()];
    while let Some(shelf) = pending.pop() {
        for (name, kind) in entries(fd, &shelf)? {
            let path = shelf.join(&name);
            let bundle = shelf.file_name().is_some_and(|name| name == "briefs");
            if plan_home(&path).is_some()
                || kind == EntryKind::Symlink
                || bundle
                || (name == "briefs" && kind != EntryKind::Directory)
                || (shelf == Path::new("doc/plans")
                    && kind == EntryKind::Directory
                    && name
                        .to_str()
                        .is_some_and(|name| is_feature_name(&name.to_ascii_lowercase())))
            {
                existing.push(path.clone());
            }
            if kind == EntryKind::Directory {
                pending.push(path);
            }
        }
    }
    Ok(())
}
