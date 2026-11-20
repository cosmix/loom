//! The outcomes of a host child's job, recorded as each criterion finishes,
//! and the logs its criteria write while they run.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::{bail, Result};

use super::child::SUMMARY_MAX_BYTES;
use super::{store, CriterionOutcome};
use crate::models::dispute::truncate_to_byte_limit;
use crate::verify::criteria::{format_criterion_result, CriterionResult, LiveOutput};

/// The label, and log name, of acceptance criterion `index` in a job.
pub(super) fn acceptance_label(index: usize) -> String {
    format!("acceptance-{index}")
}

/// The outcomes of one job, persisted as each arrives. The acceptance
/// observer cannot return an error, so the first store failure is kept and
/// turned into the job's error once the criteria finish.
pub(super) struct Progress {
    work_dir: PathBuf,
    stage_id: String,
    job_id: String,
    state: Mutex<ProgressState>,
}

#[derive(Default)]
struct ProgressState {
    outcomes: Vec<CriterionOutcome>,
    failure: Option<String>,
}

impl Progress {
    pub(super) fn new(work_dir: &Path, stage_id: &str, job_id: &str) -> Self {
        Self {
            work_dir: work_dir.to_path_buf(),
            stage_id: stage_id.to_string(),
            job_id: job_id.to_string(),
            state: Mutex::new(ProgressState::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ProgressState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record acceptance criterion `index`. A result answered from the cache
    /// captured no output and gets no log.
    pub(super) fn acceptance(&self, index: usize, result: &CriterionResult, setup: &[String]) {
        let summary = format_criterion_result(index, result, setup);
        let outcome = CriterionOutcome {
            label: acceptance_label(index),
            passed: result.success,
            summary: truncate_to_byte_limit(&summary, SUMMARY_MAX_BYTES),
            log: None,
        };
        let output =
            (!result.cached).then(|| [result.stdout.as_bytes(), result.stderr.as_bytes()].concat());
        self.record(outcome, output);
    }

    /// Copies each running criterion's output to `<job dir>/<label>.log`, the
    /// log its outcome later names, `label` giving the criterion's label. A
    /// log that cannot be opened is skipped: the outcome still records the
    /// output once the criterion ends.
    pub(super) fn live_output(&self, label: fn(usize) -> String) -> LiveOutput {
        let (work_dir, stage_id, job_id) = (
            self.work_dir.clone(),
            self.stage_id.clone(),
            self.job_id.clone(),
        );
        LiveOutput(Arc::new(move |index| {
            let log = store::open_live_log(&work_dir, &stage_id, &job_id, &label(index));
            match log {
                Ok(log) => Some(Box::new(log) as Box<dyn Write + Send>),
                Err(error) => {
                    tracing::warn!(error = %format!("{error:#}"), "no live criterion log");
                    None
                }
            }
        }))
    }

    pub(super) fn record(&self, mut outcome: CriterionOutcome, output: Option<Vec<u8>>) {
        let stored = self.store(&mut outcome, output);
        let mut state = self.lock();
        if let Err(err) = stored {
            state.failure.get_or_insert_with(|| format!("{err:#}"));
        }
        state.outcomes.push(outcome);
    }

    fn store(&self, outcome: &mut CriterionOutcome, output: Option<Vec<u8>>) -> Result<()> {
        let (work_dir, stage_id, job_id) = (&self.work_dir, &self.stage_id, &self.job_id);
        if let Some(bytes) = output {
            let log =
                store::save_criterion_log(work_dir, stage_id, job_id, &outcome.label, &bytes)?;
            outcome.log = Some(log);
        }
        store::append_progress(work_dir, stage_id, job_id, outcome)
    }

    pub(super) fn take_failure(&self) -> Result<()> {
        match self.lock().failure.take() {
            Some(failure) => bail!("cannot record the job's progress: {failure}"),
            None => Ok(()),
        }
    }

    /// Whether any outcome recorded so far failed.
    pub(super) fn any_failed(&self) -> bool {
        self.lock().outcomes.iter().any(|outcome| !outcome.passed)
    }

    pub(super) fn take_outcomes(&self) -> Vec<CriterionOutcome> {
        std::mem::take(&mut self.lock().outcomes)
    }
}
