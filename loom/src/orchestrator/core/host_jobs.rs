//! The host-job runner on the orchestrator tick (common.md 3.4): a pass of
//! the runner after the session inboxes drain, recovery once at startup, and
//! a graceful stop when the run ends. Every job the runner ends is applied
//! here (`host_verify::apply::apply_finished`), and so is every job a restart
//! interrupted or left unapplied.
//!
//! An application that fails (an I/O or state-file error a later attempt may
//! not hit) is retried at the start of each later tick, [`MAX_APPLY_ATTEMPTS`]
//! attempts in all; the last failure writes the job's `applied` marker as
//! `apply failed: <error>` so it is never retried again. The list lives in
//! memory: a crash loses it, and the startup scan of unmarked terminal jobs
//! recovers it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use anyhow::Result;

use super::Orchestrator;
use crate::host_verify::apply::{apply_finished, unapplied_terminal_records, APPLIED_MARKER};
use crate::host_verify::runner::{HostJobRunner, RunnerConfig};
use crate::host_verify::{store, JobRecord};

/// How many times one job's application is attempted.
const MAX_APPLY_ATTEMPTS: u32 = 10;

/// A job whose application failed, and how many times it has.
struct FailedApply {
    record: JobRecord,
    failures: u32,
}

/// The failed applications of each state root this process orchestrates.
/// Process-wide, keyed by the state root, because `Orchestrator` sits at the
/// file-size limit and cannot take another field; one orchestrator runs per
/// state root, so the key is its own list. Revisit if the orchestrator's
/// state moves into a struct that can hold it.
static FAILED_APPLIES: Mutex<BTreeMap<PathBuf, Vec<FailedApply>>> = Mutex::new(BTreeMap::new());

/// The runner the daemon starts with: the running loom binary's
/// `stage verify-host`. `None`, logged, when that binary cannot be located:
/// host jobs then stay disabled for this run instead of stopping it.
pub(super) fn daemon_runner(work_dir: &Path) -> Option<HostJobRunner> {
    match RunnerConfig::daemon_default() {
        Ok(config) => Some(HostJobRunner::new(work_dir.to_path_buf(), config)),
        Err(error) => {
            tracing::error!(
                error = %format!("{error:#}"),
                "host jobs are disabled for this run: no verify-host runner",
            );
            None
        }
    }
}

impl Orchestrator {
    /// One pass of the host-job runner: retry the applications that failed,
    /// then end, reap and start jobs, applying every job that ended. A runner
    /// or apply error is logged and never propagated: a broken job must not
    /// stop the orchestrator.
    pub fn tick_host_jobs(&mut self) {
        let work_dir = self.config.work_dir.clone();
        let retried = take_failed(&work_dir)
            .into_iter()
            .map(|failed| (failed.record, failed.failures));
        apply_records(&work_dir, retried);
        let Some(runner) = self.host_jobs.as_mut() else {
            return;
        };
        match runner.tick() {
            Ok(ended) => {
                log_records("host job ended", &ended);
                apply_records(&work_dir, ended.into_iter().map(|record| (record, 0)));
            }
            Err(error) => {
                tracing::warn!(error = %format!("{error:#}"), "host job tick failed");
            }
        }
    }

    /// Replace the runner `Orchestrator::new` built; the replaced one is
    /// stopped first.
    pub fn set_host_job_runner(&mut self, runner: HostJobRunner) {
        if let Some(mut replaced) = self.host_jobs.replace(runner) {
            replaced.shutdown();
        }
    }

    /// Log what `recover_after_restart` changed at startup, then apply every
    /// job it interrupted and every terminal job left unapplied, so a freeze
    /// interrupted by the restart parks its stage instead of stranding a
    /// writer that has already ended its turn.
    pub(super) fn report_host_job_recovery(&self, recovered: Result<Vec<JobRecord>>) {
        let work_dir = &self.config.work_dir;
        let mut records = match recovered {
            Ok(records) => {
                log_records("host job interrupted by the restart", &records);
                records
            }
            Err(error) => {
                tracing::warn!(
                    error = %format!("{error:#}"),
                    "could not recover host jobs after the restart",
                );
                Vec::new()
            }
        };
        match unapplied_terminal_records(work_dir) {
            Ok(unapplied) => records.extend(unapplied),
            Err(error) => tracing::warn!(
                error = %format!("{error:#}"),
                "could not list the host jobs left unapplied",
            ),
        }
        let mut seen = std::collections::BTreeSet::new();
        records.retain(|record| seen.insert(record.id.clone()));
        apply_records(work_dir, records.into_iter().map(|record| (record, 0)));
    }

    /// The graceful stop: every running job is cancelled with
    /// `daemon stopped` and its child killed.
    pub(super) fn stop_host_jobs(&mut self) {
        if let Some(runner) = self.host_jobs.as_mut() {
            runner.shutdown();
        }
    }
}

/// Apply each record, which has already failed `failures` times; keep the
/// ones that fail again for the next tick.
fn apply_records(work_dir: &Path, records: impl IntoIterator<Item = (JobRecord, u32)>) {
    let failed: Vec<FailedApply> = records
        .into_iter()
        .filter_map(|(record, failures)| apply_one(work_dir, record, failures))
        .collect();
    if failed.is_empty() {
        return;
    }
    let mut lists = FAILED_APPLIES
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    lists
        .entry(work_dir.to_path_buf())
        .or_default()
        .extend(failed);
}

fn take_failed(work_dir: &Path) -> Vec<FailedApply> {
    let mut lists = FAILED_APPLIES
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    lists.remove(work_dir).unwrap_or_default()
}

/// One application. `Some` when it failed and may be tried again.
fn apply_one(work_dir: &Path, record: JobRecord, failures: u32) -> Option<FailedApply> {
    let error = match apply_finished(work_dir, &record) {
        Ok(applied) => {
            tracing::info!(
                stage = %record.stage_id,
                job = %record.id,
                applied = ?applied,
                "host job applied",
            );
            return None;
        }
        Err(error) => error,
    };
    let failures = failures + 1;
    if failures < MAX_APPLY_ATTEMPTS {
        tracing::warn!(
            stage = %record.stage_id,
            job = %record.id,
            attempt = failures,
            error = %format!("{error:#}"),
            "host job could not be applied; retrying on the next tick",
        );
        return Some(FailedApply { record, failures });
    }
    give_up(work_dir, &record, &error);
    None
}

/// Mark a job whose every application failed, so it is never retried.
fn give_up(work_dir: &Path, record: &JobRecord, error: &anyhow::Error) {
    let marker = format!("apply failed: {error:#}");
    tracing::error!(
        stage = %record.stage_id,
        job = %record.id,
        error = %format!("{error:#}"),
        "host job could not be applied after {MAX_APPLY_ATTEMPTS} attempts",
    );
    let saved = store::save_job_file(
        work_dir,
        &record.stage_id,
        &record.id,
        APPLIED_MARKER,
        marker.as_bytes(),
    );
    if let Err(error) = saved {
        tracing::error!(
            job = %record.id,
            error = %format!("{error:#}"),
            "could not mark the host job's failed application",
        );
    }
}

fn log_records(message: &str, records: &[JobRecord]) {
    for record in records {
        tracing::info!(
            stage = %record.stage_id,
            job = %record.id,
            state = ?record.state,
            "{message}",
        );
    }
}

#[cfg(test)]
#[path = "host_jobs_tests.rs"]
mod tests;
