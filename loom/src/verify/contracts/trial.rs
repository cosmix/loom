//! A replacement trial (`loom check <stage> --contracts --replacement
//! <frozen-file>=<path>`): the stage's frozen contracts run with replacement
//! bytes laid over frozen files for the run only, and a passing trial leaves a
//! [`TrialReceipt`] that a contract dispute carrying the same bytes must match
//! before it is filed.
//!
//! [`with_overlay`] writes into the stage worktree, which the stage session
//! can also write, so every target is checked for a symlink at each component
//! before anything is written, the writes and restores go through the
//! no-follow writer, and the earlier bytes and mode come back whether the run
//! passed, failed or unwound.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::store::{validate_relative, FreezeRecord, MAX_FROZEN_FILE_BYTES};
use crate::fs::safe_read::read_bounded;
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

/// A file's bytes and mode from before the overlay.
struct Saved {
    rel: PathBuf,
    bytes: Vec<u8>,
    mode: u32,
}

/// Restores the files it holds when dropped, so an unwinding run still
/// leaves the worktree as it was.
struct Restore<'a> {
    root: &'a Path,
    saved: Vec<Saved>,
}

impl Restore<'_> {
    /// Put every saved file back; the first failure names its file, and the
    /// remaining files are still restored.
    fn finish(&mut self) -> Result<()> {
        let mut first = None;
        for saved in std::mem::take(&mut self.saved) {
            let restored = restore_no_follow(self.root, &saved.rel, &saved.bytes, saved.mode)
                .with_context(|| format!("cannot restore the frozen file {}", saved.rel.display()));
            if let Err(error) = restored {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            tracing::error!(error = %format!("{error:#}"), "a trial overlay was not restored");
        }
    }
}

/// Lay each `(path, bytes)` (relative to `worktree_root`) over its existing
/// regular file, call `run`, then restore every file's earlier bytes and mode
/// whatever `run` returned, and on unwind. A missing file, a symlinked target
/// or a symlinked parent is refused before anything is written. Returns
/// `run`'s result; a failed restore is an error naming the file.
pub fn with_overlay<T>(
    worktree_root: &Path,
    files: &[(String, Vec<u8>)],
    run: impl FnOnce() -> Result<T>,
) -> Result<T> {
    with_admitted_overlay(worktree_root, files, || Ok(()), run)
}

/// [`with_overlay`], calling `admit` after the files are checked and before
/// the first write: an `Err` writes nothing and is returned, and the value it
/// returns (a lock) is held until every file is written, then dropped before
/// `run`.
pub fn with_admitted_overlay<T, G>(
    worktree_root: &Path,
    files: &[(String, Vec<u8>)],
    admit: impl FnOnce() -> Result<G>,
    run: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let saved = files
        .iter()
        .map(|(path, _)| snapshot(worktree_root, path))
        .collect::<Result<Vec<_>>>()?;
    let admitted = admit()?;
    let mut restore = Restore {
        root: worktree_root,
        saved: Vec::with_capacity(saved.len()),
    };
    for (saved, (_, bytes)) in saved.into_iter().zip(files) {
        let written = restore_no_follow(worktree_root, &saved.rel, bytes, saved.mode)
            .with_context(|| format!("cannot lay the replacement over {}", saved.rel.display()));
        // Held before the error is returned: a partial write is restored too.
        restore.saved.push(saved);
        written?;
    }
    drop(admitted);
    let result = run();
    match (restore.finish(), result) {
        (Ok(()), result) => result,
        (Err(restored), Ok(_)) => Err(restored),
        (Err(restored), Err(run)) => Err(restored.context(format!("the run also failed: {run:#}"))),
    }
}

/// The current bytes and permission bits of the regular file `path`, which
/// must be reached without a symlink at any component.
fn snapshot(root: &Path, path: &str) -> Result<Saved> {
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
    let unfiled = receipt
        .files
        .iter()
        .any(|trial| !replacements.iter().any(|(file, _)| file == &trial.path));
    if unfiled || receipt.files.len() != replacements.len() {
        let trialled: Vec<&str> = receipt.files.iter().map(|f| f.path.as_str()).collect();
        return Err(format!(
            "the passing trial replaced {}: the filing must name the same --replacement values \
             as the trial: {RUN}",
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
        "the passing trial did not run these replacement bytes: {}: {RUN}",
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
