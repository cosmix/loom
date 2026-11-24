//! Compare-and-set changes to a stage's current host-job record, and the
//! tick step that settles a `Running` record no runner holds.

use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use super::{bounded_detail, store, JobRecord, JobState};

/// What a job that never got a child records.
pub(super) const NEVER_STARTED: &str = "verify-host never started";

/// The outcome of a compare-and-set on a job's record.
pub(super) enum Cas {
    Applied(JobRecord),
    /// The record moved on; this is the job's record as it stands, if any.
    Refused(Option<JobRecord>),
}

/// Fail every stage's current record that is `Running` with no `pid` and is
/// not `held`: the runner writes `Running` only for a job it is about to
/// spawn, holds it from the spawn on, and a previous daemon's `Running`
/// record is interrupted by startup recovery before the first tick. Returns
/// the records it failed; a failure on one stage is logged and skipped.
pub(super) fn fail_unheld_running(
    work_dir: &Path,
    held: &dyn Fn(&str) -> bool,
) -> Result<Vec<JobRecord>> {
    let mut failed = Vec::new();
    for stage_id in store::stage_ids(work_dir)? {
        match fail_if_unheld(work_dir, &stage_id, held) {
            Ok(record) => failed.extend(record),
            Err(error) => warn_stage(&stage_id, &error),
        }
    }
    Ok(failed)
}

fn fail_if_unheld(
    work_dir: &Path,
    stage_id: &str,
    held: &dyn Fn(&str) -> bool,
) -> Result<Option<JobRecord>> {
    let Some(record) = store::load_current(work_dir, stage_id)? else {
        return Ok(None);
    };
    if record.state != JobState::Running || record.pid.is_some() || held(&record.id) {
        return Ok(None);
    }
    let cas = compare_and_set(work_dir, stage_id, &record.id, JobState::Running, |r| {
        end_record(r, JobState::Failed, Some(NEVER_STARTED.to_string()));
    })?;
    Ok(match cas {
        Cas::Applied(record) => Some(record),
        Cas::Refused(_) => None,
    })
}

/// Under the stage lock, apply `change` to the job's record when it is still
/// the stage's current job in state `expected`.
pub(super) fn compare_and_set(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    expected: JobState,
    change: impl FnOnce(&mut JobRecord),
) -> Result<Cas> {
    let _lock = store::lock_stage(work_dir, stage_id)?;
    match store::load_current(work_dir, stage_id)? {
        Some(mut record) if record.id == job_id && record.state == expected => {
            change(&mut record);
            store::save_current(work_dir, &record)?;
            Ok(Cas::Applied(record))
        }
        Some(record) if record.id == job_id => Ok(Cas::Refused(Some(record))),
        _ => Ok(Cas::Refused(store::load_job_record(
            work_dir, stage_id, job_id,
        )?)),
    }
}

pub(super) fn end_record(record: &mut JobRecord, state: JobState, detail: Option<String>) {
    record.state = state;
    record.detail = detail.as_deref().map(bounded_detail);
    record.finished_at = Some(Utc::now());
}

fn warn_stage(stage_id: &str, error: &anyhow::Error) {
    tracing::warn!(
        id = %stage_id,
        error = %format!("{error:#}"),
        "could not fail a host job no runner holds"
    );
}
