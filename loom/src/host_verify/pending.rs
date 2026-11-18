//! The waiting check of the current check's session and tree.
use super::{
    store::{self, StageLock},
    submit, JobRecord,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

/// Clear terminal pointers, including a cancellation interrupted before unlink.
pub(crate) fn live_locked(
    _lock: &StageLock,
    work_dir: &Path,
    stage_id: &str,
) -> Result<Option<JobRecord>> {
    let record = store::load_pending(work_dir, stage_id)?;
    if record.as_ref().is_some_and(|job| job.state.is_terminal()) {
        store::clear_pending(work_dir, stage_id)?;
        return Ok(None);
    }
    Ok(record)
}

/// Promote after current ends; recover a crash after saving current too.
pub(crate) fn promote_locked(
    lock: &StageLock,
    work_dir: &Path,
    stage_id: &str,
) -> Result<Option<JobRecord>> {
    let Some(waiting) = live_locked(lock, work_dir, stage_id)? else {
        return Ok(None);
    };
    let current = store::load_current(work_dir, stage_id)?;
    let record = match current {
        Some(current) if current.id == waiting.id => current,
        Some(current) if !current.state.is_terminal() => return Ok(None),
        _ => {
            store::save_current(work_dir, &waiting)?;
            waiting
        }
    };
    store::clear_pending(work_dir, stage_id)?;
    Ok(Some(record).filter(|record| !record.state.is_terminal()))
}

pub(crate) fn promote(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>> {
    if store::load_pending(work_dir, stage_id)?.is_none() {
        return Ok(None);
    }
    let lock = store::lock_stage(work_dir, stage_id)?;
    promote_locked(&lock, work_dir, stage_id)
}

/// Persist cancellation before unlinking so a crash cannot orphan a live job.
pub(crate) fn cancel_locked(
    lock: &StageLock,
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<Option<JobRecord>> {
    let Some(mut record) = live_locked(lock, work_dir, stage_id)? else {
        return Ok(None);
    };
    submit::end(&mut record, reason, now);
    store::save_job_copy(work_dir, &record)?;
    store::clear_pending(work_dir, stage_id)?;
    Ok(Some(record))
}
