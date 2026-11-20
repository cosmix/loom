//! Frozen copies put back into the stage worktree: the copy reader
//! `loom stage contracts restore` uses, and the undo of a replacement trial
//! whose child was stopped while its overlay was live.
//!
//! A trial's child restores its overlay itself ([`super::trial::with_overlay`]),
//! but a superseded, cancelled or overrun job is SIGKILLed by its watchdog and
//! the runner, so no drop guard runs. The runner then calls
//! [`undo_trial_overlay`] once the child is gone: the daemon writes only bytes
//! from its own frozen copies, and only over a file that still holds the
//! trial's replacement bytes, so a file the child restored, a later edit and a
//! replacement an accepted dispute applied are all left alone.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::site::ContractSite;
use super::store::{frozen_file_path_for_record, FreezeRecord, FrozenFile, MAX_FROZEN_FILE_BYTES};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::fs::safe_replace::restore_no_follow;
use crate::models::dispute::FileReplacement;
use crate::utils::sha256_hex;

/// Read the frozen copy of `rel` and confirm it still hashes to what the
/// freeze recorded.
pub(crate) fn read_frozen(
    work_dir: &Path,
    record: &FreezeRecord,
    rel: &str,
    sha256: &str,
) -> Result<Vec<u8>> {
    let copy = frozen_file_path_for_record(work_dir, record, rel);
    let relative = copy
        .strip_prefix(work_dir)
        .with_context(|| format!("Frozen copy path {} is malformed", copy.display()))?;
    let root = work_dir.canonicalize()?;
    let content = read_bounded(&root, relative, MAX_FROZEN_FILE_BYTES)
        .with_context(|| format!("Failed to read the frozen copy {}", copy.display()))?;
    if sha256_hex(&content) != sha256 {
        bail!(
            "The frozen copy {} no longer matches the hash the freeze recorded",
            copy.display()
        );
    }
    Ok(content)
}

/// Put back the frozen copy of every file of `replacements` that still holds
/// its replacement bytes while `record` freezes other bytes. Returns the
/// restored paths, relative to the worktree root. A file that cannot be read
/// or written is an error naming it; the other files are still tried.
pub(crate) fn undo_trial_overlay(
    site: &ContractSite,
    record: &FreezeRecord,
    replacements: &[FileReplacement],
) -> Result<Vec<String>> {
    let prefix = site.working_dir_prefix();
    let mut restored = Vec::new();
    let mut first = None;
    for replacement in replacements {
        let Some(frozen) = record.files.iter().find(|f| f.path == replacement.file) else {
            continue;
        };
        let overlay = sha256_hex(replacement.content.as_bytes());
        if frozen.sha256 == overlay {
            continue;
        }
        let target = prefix.join(&replacement.file);
        let undone = undo_one(site, record, (frozen, &target), &overlay)
            .with_context(|| format!("cannot undo the trial overlay of {}", target.display()));
        match undone {
            Ok(true) => restored.push(target.to_string_lossy().into_owned()),
            Ok(false) => {}
            Err(error) => {
                first.get_or_insert(error);
            }
        }
    }
    first.map_or(Ok(restored), Err)
}

/// Restore `frozen` at `target`, its path under the worktree root, from its
/// frozen copy when the file's bytes hash to `overlay`.
fn undo_one(
    site: &ContractSite,
    record: &FreezeRecord,
    (frozen, target): (&FrozenFile, &Path),
    overlay: &str,
) -> Result<bool> {
    let root = &site.worktree_root;
    let current = match read_bounded(root, target, MAX_FROZEN_FILE_BYTES) {
        Ok(current) => current,
        Err(error) if is_not_found(&error) => return Ok(false),
        Err(error) => return Err(error),
    };
    if sha256_hex(&current) != overlay {
        return Ok(false);
    }
    let content = read_frozen(&site.work_dir, record, &frozen.path, &frozen.sha256)?;
    // The overlay kept the file's mode; setuid, setgid and sticky bits are dropped.
    let mode = std::fs::symlink_metadata(root.join(target))?
        .permissions()
        .mode()
        & 0o777;
    restore_no_follow(root, target, &content, mode)?;
    Ok(true)
}

#[cfg(test)]
#[path = "frozen_restore_tests.rs"]
mod tests;
