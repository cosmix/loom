//! Detect configuration edits made while Codex reviews a plan.

use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_read::{list_dir_no_follow, read_bounded, EntryKind};

// Fail closed above this per-file limit; hashing a prefix would hide edits.
const MAX_CONFIG_BYTES: usize = 4 * 1024 * 1024;

/// SHA-256 digests indexed by sorted repository-relative configuration paths.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigSnapshot {
    files: BTreeMap<String, String>,
}

/// Snapshot regular files in `.claude/`, plus root `.mcp.json` and `CLAUDE.md`.
/// Symlinks and special files are skipped; unsafe races and read errors fail.
pub fn snapshot(root: &Path) -> anyhow::Result<ConfigSnapshot> {
    let anchor = safe_open_dirfd(root)?;
    let mut snapshot = ConfigSnapshot {
        files: BTreeMap::new(),
    };
    for (name, kind) in list_dir_no_follow(&anchor, Path::new(""))? {
        let relative = PathBuf::from(name);
        if relative == Path::new(".claude") && kind == EntryKind::Directory {
            snapshot_claude(root, &anchor, &mut snapshot)?;
        } else if matches!(relative.to_str(), Some(".mcp.json" | "CLAUDE.md"))
            && kind == EntryKind::File
        {
            hash_file(root, &relative, &mut snapshot)?;
        }
    }
    Ok(snapshot)
}

/// Return added, removed, and modified paths in repository-relative order.
pub fn changed(before: &ConfigSnapshot, after: &ConfigSnapshot) -> Vec<String> {
    before
        .files
        .keys()
        .chain(after.files.keys())
        .filter(|path| before.files.get(*path) != after.files.get(*path))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn snapshot_claude(root: &Path, anchor: &OwnedFd, snapshot: &mut ConfigSnapshot) -> Result<()> {
    let mut directories = vec![PathBuf::from(".claude")];
    while let Some(directory) = directories.pop() {
        let entries = list_dir_no_follow(anchor, &directory)
            .with_context(|| format!("Cannot snapshot {}", directory.display()))?;
        for (name, kind) in entries {
            let relative = directory.join(name);
            match kind {
                EntryKind::Directory => directories.push(relative),
                EntryKind::File => hash_file(root, &relative, snapshot)?,
                EntryKind::Symlink | EntryKind::Special => {}
            }
        }
    }
    Ok(())
}

fn hash_file(root: &Path, relative: &Path, snapshot: &mut ConfigSnapshot) -> Result<()> {
    let path = relative
        .to_str()
        .with_context(|| format!("Configuration path is not UTF-8: {}", relative.display()))?;
    let bytes = read_bounded(root, relative, MAX_CONFIG_BYTES)
        .with_context(|| format!("Cannot snapshot {path}"))?;
    snapshot
        .files
        .insert(path.to_owned(), hex::encode(Sha256::digest(&bytes)));
    Ok(())
}

#[cfg(test)]
mod tests;
