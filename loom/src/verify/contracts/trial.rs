//! A replacement trial (`loom check <stage> --contracts --replacement
//! <frozen-file>=<path>`): the stage's frozen contracts run with replacement
//! bytes laid over frozen files for the run only, and a passing trial leaves a
//! [`TrialReceipt`] that a contract dispute carrying the same bytes must match
//! before it is filed. The receipt and its checks live in `trial_receipt.rs`.
//!
//! [`with_overlay`] writes into the stage worktree, which the stage session
//! can also write, so every target is checked for a symlink at each component
//! before anything is written, the writes and restores go through the
//! no-follow writer, and the restore runs whether the run passed, failed or
//! unwound. The restore is conditional: under the stage lock it writes a saved
//! file back only if the file still holds its overlay bytes and the freeze does
//! not record it at those bytes; a missing file stays missing and a file
//! holding other bytes is left alone.

use std::marker::PhantomData;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::store::{validate_relative, FreezeRecord, MAX_FROZEN_FILE_BYTES};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::fs::safe_replace::{open_parent, restore_no_follow};
use crate::models::dispute::FileReplacement;
use crate::utils::sha256_hex;

#[path = "trial_receipt.rs"]
mod receipt;

pub use receipt::{
    current_tree, load_receipt, receipt_covers, receipt_files_cover, trial_files, TrialFile,
    TrialReceipt, RECEIPT_FILE,
};

/// Refuse a trial of a file `record`, the stage's freeze, does not hold.
pub fn refuse_unfrozen(
    record: &FreezeRecord,
    replacements: &[FileReplacement],
) -> Result<(), String> {
    let outside = replacements
        .iter()
        .find(|r| !record.files.iter().any(|file| file.path == r.file));
    match outside {
        Some(outside) => Err(format!(
            "'{}' is not a frozen file of stage '{}': a trial replaces frozen files only",
            outside.file, record.stage_id
        )),
        None => Ok(()),
    }
}

/// A file's bytes and mode from before the overlay, and the sha256 of the
/// replacement laid over it.
struct Saved {
    rel: PathBuf,
    bytes: Vec<u8>,
    mode: u32,
    overlay: String,
}

/// What a trial's restore runs under: a lock, and the `(path relative to the
/// worktree root, sha256)` each frozen file is frozen at, read under it.
pub type Relocked<G> = (G, Vec<(PathBuf, String)>);

/// Restores the files it holds when dropped, so an unwinding run still
/// leaves the worktree as it was. `lock` is taken for the restore: the
/// writers that may replace and re-freeze a frozen file during the run (an
/// accepted contract ruling) hold the same lock.
struct Restore<'a, L: Fn() -> Result<Relocked<G>>, G> {
    root: &'a Path,
    saved: Vec<Saved>,
    lock: L,
    _guard: PhantomData<fn() -> G>,
}

impl<L: Fn() -> Result<Relocked<G>>, G> Restore<'_, L, G> {
    /// Put back every saved file that still holds its overlay bytes while the
    /// freeze holds other bytes for it. A file holding anything else was
    /// written during the run (an edit, which the run's drift check reports),
    /// and one frozen at the overlay bytes was replaced and re-frozen by an
    /// accepted ruling: both are left alone. The first failure names its
    /// file, and the remaining files are still tried.
    fn finish(&mut self) -> Result<()> {
        if self.saved.is_empty() {
            return Ok(());
        }
        let (_guard, frozen) =
            (self.lock)().context("cannot lock the stage to restore a trial overlay")?;
        let mut first = None;
        for saved in std::mem::take(&mut self.saved) {
            let refrozen = frozen
                .iter()
                .any(|(rel, sha256)| *rel == saved.rel && *sha256 == saved.overlay);
            if refrozen {
                continue;
            }
            let restored = restore_if_overlaid(self.root, &saved)
                .with_context(|| format!("cannot restore the frozen file {}", saved.rel.display()));
            if let Err(error) = restored {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
}

impl<L: Fn() -> Result<Relocked<G>>, G> Drop for Restore<'_, L, G> {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            tracing::error!(error = %format!("{error:#}"), "a trial overlay was not restored");
        }
    }
}

/// Write `saved`'s earlier bytes back when the file still hashes to its
/// overlay; a missing file is left missing.
fn restore_if_overlaid(root: &Path, saved: &Saved) -> Result<()> {
    let current = match read_bounded(root, &saved.rel, MAX_FROZEN_FILE_BYTES) {
        Ok(current) => current,
        Err(error) if is_not_found(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    if sha256_hex(&current) != saved.overlay {
        return Ok(());
    }
    restore_no_follow(root, &saved.rel, &saved.bytes, saved.mode)
}

/// Lay each `(path, bytes)` (relative to `worktree_root`) over its existing
/// regular file, call `run`, then restore each file's earlier bytes and mode
/// whatever `run` returned, and on unwind. The restore is conditional: under
/// the stage lock a saved file is written back only if it still holds its
/// overlay bytes and the freeze does not record it at those bytes; a missing
/// file stays missing and a file holding other bytes is left alone. A missing
/// file, a symlinked target or a symlinked parent is refused before anything
/// is written. Returns `run`'s result; a failed restore is an error naming
/// the file.
pub fn with_overlay<T>(
    worktree_root: &Path,
    files: &[(String, Vec<u8>)],
    run: impl FnOnce() -> Result<T>,
) -> Result<T> {
    with_admitted_overlay(
        worktree_root,
        files,
        (|| Ok(()), || Ok(((), Vec::new()))),
        run,
    )
}

/// [`with_overlay`], calling `admit` before the files are checked and read:
/// an `Err` writes nothing and is returned, and the value it returns (a lock)
/// is held until every file is written, then dropped before `run`. The earlier
/// bytes are read under it, so an overlay another holder of the lock lays or
/// undoes is never saved halfway as the file's original. `relock` is held
/// while the files are restored, and a file is restored only while it holds
/// its replacement bytes and is not frozen at them, so a frozen file another
/// holder of the lock rewrote or re-froze during the run keeps that state.
pub fn with_admitted_overlay<T, G, H>(
    worktree_root: &Path,
    files: &[(String, Vec<u8>)],
    (admit, relock): (impl FnOnce() -> Result<G>, impl Fn() -> Result<Relocked<H>>),
    run: impl FnOnce() -> Result<T>,
) -> Result<T> {
    // Declared before the admission so an unwind drops the admission's lock
    // first: the restore takes the same lock.
    let mut restore = Restore {
        root: worktree_root,
        saved: Vec::with_capacity(files.len()),
        lock: relock,
        _guard: PhantomData,
    };
    let admitted = admit()?;
    let laid = lay(worktree_root, files, &mut restore.saved);
    drop(admitted);
    laid?;
    let result = run();
    match (restore.finish(), result) {
        (Ok(()), result) => result,
        (Err(restored), Ok(_)) => Err(restored),
        (Err(restored), Err(run)) => Err(restored.context(format!("the run also failed: {run:#}"))),
    }
}

/// Snapshot every file, then write each replacement, pushing a file onto
/// `saved` once its replacement is written; nothing is written unless every
/// snapshot succeeds.
fn lay(root: &Path, files: &[(String, Vec<u8>)], saved: &mut Vec<Saved>) -> Result<()> {
    let snapshots = files
        .iter()
        .map(|(path, bytes)| snapshot(root, path, bytes))
        .collect::<Result<Vec<_>>>()?;
    for (snapshot, (_, bytes)) in snapshots.into_iter().zip(files) {
        // The write is atomic (a rename), so a failed one leaves the file as it was.
        restore_no_follow(root, &snapshot.rel, bytes, snapshot.mode).with_context(|| {
            format!("cannot lay the replacement over {}", snapshot.rel.display())
        })?;
        saved.push(snapshot);
    }
    Ok(())
}

/// The current bytes and permission bits of the regular file `path`, which
/// must be reached without a symlink at any component, saved to be put back
/// over `overlay`.
fn snapshot(root: &Path, path: &str, overlay: &[u8]) -> Result<Saved> {
    validate_relative(path)?;
    let rel = PathBuf::from(path);
    let refused = || format!("refusing to trial a replacement of {path}");
    open_parent(root, &rel).with_context(refused)?;
    let metadata = std::fs::symlink_metadata(root.join(&rel)).with_context(refused)?;
    if !metadata.is_file() {
        bail!("{}: it is not a regular file", refused());
    }
    let bytes = read_bounded(root, &rel, MAX_FROZEN_FILE_BYTES).with_context(refused)?;
    Ok(Saved {
        rel,
        bytes,
        // Setuid, setgid and sticky bits are not carried onto the rewrite.
        mode: metadata.permissions().mode() & 0o777,
        overlay: sha256_hex(overlay),
    })
}

#[cfg(test)]
#[path = "trial_receipt_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trial_overlay_tests.rs"]
mod overlay_tests;
