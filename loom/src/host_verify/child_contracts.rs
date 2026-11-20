//! The contracts a check runs: `loom check --contracts` runs only the
//! stage's frozen contracts, a trial (`--replacement`) runs them with the
//! job's replacement files laid over frozen files, and a check of every
//! criterion runs them after the goal-backward checks. Each contract is an
//! outcome `contract:<id>`, judged as the completion check judges it.

use std::path::Path;

use anyhow::{Context, Result};

use super::child::{load_running, JobTree, Progress};
use super::freeze::format_outcome;
use super::submit::REPLACEMENTS_FILE;
use super::{store, CriterionOutcome, JobRecord, JobState};
use crate::models::dispute::{truncate_to_byte_limit, FileReplacement};
use crate::models::stage::Stage;
use crate::verify::contracts::completion::{self, ContractCheck};
use crate::verify::contracts::drift::{drift_lines, expecting};
use crate::verify::contracts::frozen_restore::undo_trial_overlay;
use crate::verify::contracts::site::{stage_site, ContractSite};
use crate::verify::contracts::store::load_freeze;
use crate::verify::contracts::trial::{
    self, trial_files, with_admitted_overlay, Relocked, TrialFile, TrialReceipt, RECEIPT_FILE,
};
use crate::verify::criteria::{plan_confinement, resolve_confinement};
use crate::verify::transitions::load_stage;

/// A `--contracts` job on the tree `tree` already matched to `record`: the
/// frozen contracts, or a trial of the job's replacement files. Returns how
/// the tree changed during the run, as the check pipeline does.
pub(super) fn run(
    work_dir: &Path,
    record: &JobRecord,
    stage: &Stage,
    tree: &JobTree,
    progress: &Progress,
) -> Result<Option<String>> {
    let replacements = load_replacements(work_dir, &record.stage_id, &record.id)?;
    if replacements.is_empty() {
        record_check(progress, &contract_check(stage, tree, work_dir, &[])?);
        progress.take_failure()?;
        return tree.differences(record);
    }
    trial(work_dir, record, (stage, tree), &replacements, progress)
}

/// The contracts of a check of every criterion, on a stage whose contracts a
/// full check runs (`completion::runs_in_full_check`).
pub(super) fn run_full(
    stage: &Stage,
    tree: &JobTree,
    work_dir: &Path,
    progress: &Progress,
) -> Result<()> {
    if completion::runs_in_full_check(stage, work_dir)? {
        record_check(progress, &contract_check(stage, tree, work_dir, &[])?);
    }
    Ok(())
}

fn load_replacements(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
) -> Result<Vec<FileReplacement>> {
    let Some(bytes) = store::load_job_file(work_dir, stage_id, job_id, REPLACEMENTS_FILE)? else {
        return Ok(Vec::new());
    };
    serde_json::from_slice(&bytes).with_context(|| format!("{REPLACEMENTS_FILE} is malformed"))
}

fn contract_check(
    stage: &Stage,
    tree: &JobTree,
    work_dir: &Path,
    overrides: &[TrialFile],
) -> Result<ContractCheck> {
    let root = worktree_root(tree)?;
    let acceptance_dir = tree.paths.acceptance_dir.as_deref().unwrap_or(root);
    completion::run(stage, work_dir, acceptance_dir, root, overrides)
}

fn worktree_root(tree: &JobTree) -> Result<&Path> {
    tree.paths
        .worktree_root
        .as_deref()
        .context("the contracts run in the stage worktree, and this stage has none")
}

/// One outcome per contract, after a `frozen-files` outcome when a frozen
/// file is not as the check expects.
fn record_check(progress: &Progress, check: &ContractCheck) {
    if !check.drift.is_empty() {
        progress.record(
            outcome("frozen-files", false, &check.drift.join("\n")),
            None,
        );
    }
    for contract in &check.contracts {
        let mut summary = if contract.failures.is_empty() {
            format!("contract '{}' passes", contract.id)
        } else {
            contract.failures.join("\n")
        };
        if let Some(marker) = contract.marker {
            summary.push_str(&format!(
                "\nThe output names a restriction of the confined runner ('{marker}'): advice \
                 only, it changes no result."
            ));
        }
        let label = format!("contract:{}", contract.id);
        progress.record(
            outcome(&label, contract.failures.is_empty(), &summary),
            None,
        );
    }
}

fn outcome(label: &str, passed: bool, summary: &str) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed,
        summary: truncate_to_byte_limit(summary, super::child::SUMMARY_MAX_BYTES),
        log: None,
    }
}

/// Lay `replacements` over their frozen files, run the contracts and the
/// freeze's format and static gates on them, restore the frozen files, and
/// on a pass on an unchanged tree save the job's [`TrialReceipt`].
fn trial(
    work_dir: &Path,
    record: &JobRecord,
    (stage, tree): (&Stage, &JobTree),
    replacements: &[FileReplacement],
    progress: &Progress,
) -> Result<Option<String>> {
    refuse_unfrozen(work_dir, stage, replacements)?;
    let root = worktree_root(tree)?;
    let working_dir = tree.paths.acceptance_dir.as_deref().unwrap_or(root);
    let site = ContractSite::resolved(
        work_dir.to_path_buf(),
        stage.clone(),
        root.to_path_buf(),
        working_dir.to_path_buf(),
    );
    let (files, written) = overlay_files(&site, replacements);
    let overrides = trial_files(replacements);
    let confinement = resolve_confinement(
        stage.sandbox.command_confinement,
        plan_confinement(work_dir),
    );
    let admit = || admit_overlay(work_dir, record);
    let relock = || relock_frozen(work_dir, &stage.id, &site.working_dir_prefix());
    let (check, gate) = with_admitted_overlay(root, &files, (admit, relock), || {
        let mut check = contract_check(stage, tree, work_dir, &overrides)?;
        let gate = format_outcome(&site, &tree.git, confinement, &written, check.passed())?;
        for line in drift_after_runs(work_dir, &stage.id, (working_dir, root), &overrides)? {
            if !check.drift.contains(&line) {
                check.drift.push(line);
            }
        }
        Ok((check, gate))
    })?;
    record_check(progress, &check);
    progress.record(gate, None);
    progress.take_failure()?;
    let change = tree.differences(record)?;
    if change.is_none() && !progress.any_failed() {
        save_receipt(work_dir, record, overrides)?;
    }
    Ok(change)
}

/// The `(path relative to the worktree root, bytes)` the trial lays over,
/// and every form a tool may name a replaced file by, as the freeze's gate
/// takes them.
fn overlay_files(
    site: &ContractSite,
    replacements: &[FileReplacement],
) -> (Vec<(String, Vec<u8>)>, Vec<String>) {
    let prefix = site.working_dir_prefix();
    let in_root = |file: &str| prefix.join(file).to_string_lossy().into_owned();
    let files = replacements
        .iter()
        .map(|r| (in_root(&r.file), r.content.clone().into_bytes()))
        .collect();
    let written = replacements
        .iter()
        .flat_map(|r| {
            let absolute = site
                .working_dir
                .join(&r.file)
                .to_string_lossy()
                .into_owned();
            [r.file.clone(), in_root(&r.file), absolute]
        })
        .collect();
    (files, written)
}

/// The stage lock a trial restores its overlay under, and what the freeze
/// holds then for each file, by its path under the worktree root: an
/// accepted contract ruling rewrites and re-freezes a file under this lock
/// (`apply_contract_rulings`), and the restore must not undo it.
fn relock_frozen(
    work_dir: &Path,
    stage_id: &str,
    prefix: &Path,
) -> Result<Relocked<store::StageLock>> {
    let lock = store::lock_stage(work_dir, stage_id)?;
    let frozen = load_freeze(work_dir, stage_id)?
        .map(|record| {
            let files = record.files.into_iter();
            files.map(|f| (prefix.join(&f.path), f.sha256)).collect()
        })
        .unwrap_or_default();
    Ok((lock, frozen))
}

/// The frozen files that, once the runs ended, no longer hold what the trial
/// expects: its replacement for an overlaid file, the frozen bytes for every
/// other one. Read before the overlay's restore, which would erase a change
/// made during the runs, so a receipt never certifies bytes that did not run.
/// An edit made and reverted within the runs is not seen.
fn drift_after_runs(
    work_dir: &Path,
    stage_id: &str,
    (acceptance_dir, worktree_root): (&Path, &Path),
    overrides: &[TrialFile],
) -> Result<Vec<String>> {
    let record = load_freeze(work_dir, stage_id)?
        .with_context(|| format!("the contracts of stage '{stage_id}' are no longer frozen"))?;
    drift_lines(&expecting(record, overrides), acceptance_dir, worktree_root)
}

/// A trial replaces only files the freeze record holds.
fn refuse_unfrozen(work_dir: &Path, stage: &Stage, replacements: &[FileReplacement]) -> Result<()> {
    let record = load_freeze(work_dir, &stage.id)?
        .with_context(|| format!("the contracts of stage '{}' were never frozen", stage.id))?;
    trial::refuse_unfrozen(&record, replacements).map_err(anyhow::Error::msg)
}

/// Admit a trial's first write only while `record` is still the stage's
/// running job, under the stage lock held until the overlay is written. The
/// daemon marks a job stopped under that lock before it undoes the job's
/// overlay (`stopped_overlay`), so a child it stopped either never writes or
/// wrote before the undo; a daemon restart need not wait for the watchdog.
pub(super) fn admit_overlay(work_dir: &Path, record: &JobRecord) -> Result<store::StageLock> {
    let lock = store::lock_stage(work_dir, &record.stage_id)?;
    load_running(work_dir, &record.stage_id, &record.id)
        .context("a trial no longer running must not lay its replacements over frozen files")?;
    Ok(lock)
}

/// Save the receipt of a passing trial that is still the stage's running job.
/// A job withdrawn meanwhile proves nothing for a filing; `load_receipt` also
/// skips the receipt of every job that was withdrawn, failed or interrupted.
fn save_receipt(work_dir: &Path, record: &JobRecord, files: Vec<TrialFile>) -> Result<()> {
    if load_running(work_dir, &record.stage_id, &record.id).is_err() {
        return Ok(());
    }
    let receipt = TrialReceipt {
        head: record.head.clone(),
        files,
    };
    let bytes = serde_json::to_vec(&receipt).context("cannot serialize the trial receipt")?;
    store::save_job_file(work_dir, &record.stage_id, &record.id, RECEIPT_FILE, &bytes)
}

/// After job `job_id`'s child stopped, put back every frozen file it left
/// overlaid: a withdrawn or overrun trial is SIGKILLed, so its overlay's own
/// restore never ran (`frozen_restore`). A job without replacements has
/// nothing to undo; a failure is logged, as retirement goes on regardless.
pub(super) fn undo_stopped_overlay(work_dir: &Path, stage_id: &str, job_id: &str) {
    match stopped_overlay(work_dir, stage_id, job_id) {
        Ok(restored) => {
            for path in restored {
                tracing::warn!(stage = %stage_id, job = %job_id, file = %path,
                    "restored a frozen file a stopped trial left overlaid");
            }
        }
        Err(error) => tracing::error!(stage = %stage_id, job = %job_id,
            error = %format!("{error:#}"), "could not undo a stopped trial's overlay"),
    }
}

fn stopped_overlay(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Vec<String>> {
    let replacements = load_replacements(work_dir, stage_id, job_id)?;
    if replacements.is_empty() {
        return Ok(Vec::new());
    }
    let Some(freeze) = load_freeze(work_dir, stage_id)? else {
        return Ok(Vec::new());
    };
    let stage = load_stage(stage_id, work_dir)?;
    let (worktree_root, working_dir) = stage_site(work_dir, &stage)?;
    let site = ContractSite::resolved(work_dir.to_path_buf(), stage, worktree_root, working_dir);
    // Under the lock a newer trial takes for its writes (`admit_overlay`).
    let _lock = store::lock_stage(work_dir, stage_id)?;
    let live = live_overlay(work_dir, stage_id, job_id)?;
    let ours: Vec<FileReplacement> = replacements
        .into_iter()
        .filter(|r| !live.iter().any(|newer| newer.file == r.file))
        .collect();
    undo_trial_overlay(&site, &freeze, &ours)
}

/// The replacements the stage's running job, when it is a job other than
/// `job_id`, has laid or may lay over frozen files: retiring a stale job must
/// not revert a newer trial's live overlay of the same bytes.
fn live_overlay(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Vec<FileReplacement>> {
    match store::load_current(work_dir, stage_id)? {
        Some(current) if current.id != job_id && current.state == JobState::Running => {
            load_replacements(work_dir, stage_id, &current.id)
        }
        _ => Ok(Vec::new()),
    }
}
