//! A replacement trial (`loom check <stage> --contracts --replacement
//! <frozen-file>=<path>`): the stage's frozen contracts run with replacement
//! bytes laid over frozen files for the run only, and a passing trial leaves a
//! [`TrialReceipt`] that a contract dispute carrying the same bytes must match
//! before it is filed.
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
use serde::{Deserialize, Serialize};

use super::store::{validate_relative, FreezeRecord, MAX_FROZEN_FILE_BYTES};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::fs::safe_replace::{open_parent, restore_no_follow};
use crate::host_verify::{store as job_store, JobKind, JobRecord, JobState};
use crate::models::dispute::FileReplacement;
use crate::utils::sha256_hex;

/// The job file a passing trial saves its receipt as.
pub const RECEIPT_FILE: &str = "trial.json";

/// What a passing trial proved: the replacement bytes, by sha256, passed the
/// contracts at `head`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialReceipt {
    pub head: String,
    pub files: Vec<TrialFile>,
}

/// One replaced frozen file, in the freeze record's path form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialFile {
    pub path: String,
    pub sha256: String,
}

/// The receipt form of `replacements`: each frozen file with the sha256 of
/// its replacement bytes.
pub fn trial_files(replacements: &[FileReplacement]) -> Vec<TrialFile> {
    replacements
        .iter()
        .map(|replacement| TrialFile {
            path: replacement.file.clone(),
            sha256: sha256_hex(replacement.content.as_bytes()),
        })
        .collect()
}

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

/// Admit a filing's `(frozen file, sha256)` replacements only when a passing
/// trial at `head` ran exactly that set: the same bytes for each file and no
/// other file, because a replacement may pass only alongside another one.
pub fn receipt_covers(
    receipt: Option<&TrialReceipt>,
    head: &str,
    replacements: &[(String, String)],
) -> Result<(), String> {
    const RUN: &str = "run `loom check <stage> --contracts --replacement <frozen-file>=<path>` \
                       with the same replacements until it passes, then file the dispute";
    let Some(receipt) = receipt else {
        return Err(format!("no passing replacement trial is recorded: {RUN}"));
    };
    if receipt.head != head {
        return Err(format!(
            "the passing trial ran at HEAD {}, the stage is at {head}: {RUN}",
            receipt.head
        ));
    }
    receipt_files_cover(receipt, replacements).map_err(|problem| format!("{problem}: {RUN}"))
}

/// Whether `receipt` ran exactly the `(frozen file, sha256)` set
/// `replacements`, whatever HEAD it ran at; the error says how it differs.
pub fn receipt_files_cover(
    receipt: &TrialReceipt,
    replacements: &[(String, String)],
) -> Result<(), String> {
    let unfiled = receipt
        .files
        .iter()
        .any(|trial| !replacements.iter().any(|(file, _)| file == &trial.path));
    if unfiled || receipt.files.len() != replacements.len() {
        let trialled: Vec<&str> = receipt.files.iter().map(|f| f.path.as_str()).collect();
        return Err(format!(
            "the passing trial replaced {}: the filing must name the same --replacement values \
             as the trial",
            trialled.join(", ")
        ));
    }
    let uncovered: Vec<&str> = replacements
        .iter()
        .filter(|(file, sha256)| {
            !receipt
                .files
                .iter()
                .any(|trial| &trial.path == file && &trial.sha256 == sha256)
        })
        .map(|(file, _)| file.as_str())
        .collect();
    if uncovered.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the passing trial did not run these replacement bytes: {}",
        uncovered.join(", ")
    ))
}

/// The receipt of the stage's newest passing trial still in the job store,
/// by submission order. A job that was withdrawn, failed or interrupted
/// counts for nothing whatever files it left; a malformed receipt is skipped
/// with a warning, so one bad file cannot refuse every filing.
pub fn load_receipt(work_dir: &Path, stage_id: &str) -> Result<Option<TrialReceipt>> {
    let jobs = match job_store::job_ids(work_dir, stage_id) {
        Ok(jobs) => jobs,
        Err(error) if crate::fs::safe_read::is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut newest: Option<(u64, TrialReceipt)> = None;
    for job_id in jobs {
        let Some(record) = job_store::load_job_record(work_dir, stage_id, &job_id)? else {
            continue;
        };
        let newer = newest
            .as_ref()
            .is_none_or(|(order, _)| record.submission_order > *order);
        if !newer || !passed_trial(work_dir, &record)? {
            continue;
        }
        let Some(bytes) = job_store::load_job_file(work_dir, stage_id, &job_id, RECEIPT_FILE)?
        else {
            continue;
        };
        match serde_json::from_slice::<TrialReceipt>(&bytes) {
            Ok(receipt) => newest = Some((record.submission_order, receipt)),
            Err(error) => tracing::warn!(stage = %stage_id, job = %job_id, error = %error,
                "skipping a malformed trial receipt"),
        }
    }
    Ok(newest.map(|(_, receipt)| receipt))
}

/// Whether `record` is a contracts check that passed. The child writes its
/// result before the runner settles the record, so `Running` with a passing
/// result counts; a withdrawn (`Cancelled`) job never does.
fn passed_trial(work_dir: &Path, record: &JobRecord) -> Result<bool> {
    if record.kind != JobKind::Check
        || !record.contracts
        || !matches!(record.state, JobState::Passed | JobState::Running)
    {
        return Ok(false);
    }
    Ok(
        job_store::load_result(work_dir, &record.stage_id, &record.id)?
            .is_some_and(|result| result.passed),
    )
}

#[cfg(test)]
#[path = "trial_receipt_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trial_overlay_tests.rs"]
mod overlay_tests;
