//! The receipt a passing replacement trial leaves, and the check a filing
//! must pass against it: the same replacement bytes, ran at the HEAD and on
//! the tree digest the stage holds when the dispute is filed.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::git::worktree::WorktreeGit;
use crate::host_verify::{digest, store as job_store, JobKind, JobRecord, JobState};
use crate::models::dispute::FileReplacement;
use crate::models::stage::{Stage, StageType};
use crate::utils::sha256_hex;

/// The job file a passing trial saves its receipt as.
pub const RECEIPT_FILE: &str = "trial.json";

/// What a passing trial proved: the replacement bytes, by sha256, passed the
/// contracts at `head` on the worktree `tree_digest` names, uncommitted edits
/// included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialReceipt {
    pub head: String,
    pub tree_digest: String,
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

/// The HEAD and tree digest of `stage`'s tree read through `git`, as the
/// trial's host job compares them (`JobTree::differences`).
pub fn current_tree(git: &WorktreeGit, stage: &Stage) -> Result<(String, String)> {
    let head = digest::head_in(git)?;
    let tree_digest = if stage.stage_type == StageType::Knowledge {
        digest::knowledge_tree_digest_in(git)?
    } else {
        digest::tree_digest_in(git)?
    };
    Ok((head, tree_digest))
}

/// Admit a filing's `(frozen file, sha256)` replacements only when a passing
/// trial at `head` on the tree `tree_digest` ran exactly that set: the same
/// bytes for each file and no other file, because a replacement may pass only
/// alongside another one.
pub fn receipt_covers(
    receipt: Option<&TrialReceipt>,
    head: &str,
    tree_digest: &str,
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
    if receipt.tree_digest != tree_digest {
        return Err(format!(
            "the passing trial ran on a different tree than the worktree holds now (its \
             uncommitted changes differ): {RUN}"
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

/// Whether `record` is a contracts check that passed. Only a job the runner
/// settled as passed counts: `loom check` returns only on a terminal state
/// (`host_verify::wait`), so a filing after it sees the settled record, and a
/// `Running` job with a passing result may still be withdrawn or fail its
/// remaining work.
fn passed_trial(work_dir: &Path, record: &JobRecord) -> Result<bool> {
    if record.kind != JobKind::Check || !record.contracts || record.state != JobState::Passed {
        return Ok(false);
    }
    Ok(
        job_store::load_result(work_dir, &record.stage_id, &record.id)?
            .is_some_and(|result| result.passed),
    )
}
