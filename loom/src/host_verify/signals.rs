//! Retirement and restart recovery for host verification jobs.
//!
//! Criterion groups have persistent guardians. Loom writes to each private
//! control FIFO; the guardian signals its own group. Stored PGIDs are never
//! sent a signal, because they may have been recycled after a leader exited.

use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use super::{store, JobRecord, JobState};
use crate::process::OwnedProcess;

const RESTART_DETAIL: &str = "daemon restarted during the run";

pub(super) fn retire(work_dir: &Path, stage_id: &str, job_id: &str, process: &mut OwnedProcess) {
    process.retire();
    kill_groups(work_dir, stage_id, job_id);
}

/// Ask every recorded guardian to stop its own group. Legacy entries without
/// a control endpoint fail closed; they cannot safely name a process group.
pub(super) fn kill_groups(work_dir: &Path, stage_id: &str, job_id: &str) {
    let groups = match store::load_group_anchors(work_dir, stage_id, job_id) {
        Ok(groups) => groups,
        Err(error) => {
            tracing::warn!(stage = %stage_id, job = %job_id,
                error = %format!("{error:#}"), "could not read criterion group controls");
            return;
        }
    };
    if let Err(error) = report_uncontrolled_groups(work_dir, stage_id, job_id) {
        tracing::warn!(stage = %stage_id, job = %job_id,
            error = %format!("{error:#}"), "could not persist unresolved criterion retirement");
    }
    for group in groups {
        if let Err(error) = group.signal() {
            tracing::warn!(stage = %stage_id, job = %job_id,
                error = %format!("{error:#}"), "criterion group may survive retirement");
        }
    }
}

/// Old records contain no capability for identifying their surviving commands.
/// Persist that uncertainty across restarts and keep the original job evidence.
fn report_uncontrolled_groups(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<()> {
    let groups = store::load_uncontrolled_groups(work_dir, stage_id, job_id)?;
    if groups.is_empty() {
        return Ok(());
    }
    let report = serde_json::json!({
        "unverified_process_groups": groups,
        "reason": "legacy group records have no identity-bound guardian control",
        "action": "inspect the retained job logs and establish live process ownership before stopping any process; recorded PGIDs alone are unsafe to signal",
    });
    store::save_job_file(
        work_dir,
        stage_id,
        job_id,
        "retirement-pending.json",
        &serde_json::to_vec_pretty(&report)?,
    )?;
    let path = store::job_dir(work_dir, stage_id, job_id)?.join("retirement-pending.json");
    tracing::warn!(stage = %stage_id, job = %job_id, report = %path.display(),
        "legacy criterion retirement requires ownership review; processes and evidence retained");
    Ok(())
}

enum Found {
    Interrupted(JobRecord),
    Retry(JobRecord),
    Cancelled(JobRecord),
}

/// Interrupt running jobs. Their watchdogs stop the host-child groups, and
/// their criterion guardians stop the separately grouped commands.
/// Cancelled jobs are retried too: a private FIFO has no reader after prior
/// retirement, so repeating it cannot reach an unrelated process group.
pub fn recover_after_restart(work_dir: &Path) -> Result<Vec<JobRecord>> {
    let (mut interrupted, mut retry, mut cancelled) = (Vec::new(), Vec::new(), Vec::new());
    for stage_id in store::stage_ids(work_dir)? {
        match interrupt(work_dir, &stage_id) {
            Ok(Some(Found::Interrupted(record))) => interrupted.push(record),
            Ok(Some(Found::Retry(record))) => retry.push(record),
            Ok(Some(Found::Cancelled(record))) => cancelled.push(record),
            Ok(None) => {}
            Err(error) => tracing::warn!(stage = %stage_id,
                error = %format!("{error:#}"), "could not interrupt host job"),
        }
        let current = store::load_current(work_dir, &stage_id)
            .ok()
            .flatten()
            .map(|record| record.id);
        if let Err(error) = retire_historical_groups(work_dir, &stage_id, current.as_deref()) {
            tracing::warn!(stage = %stage_id, error = %format!("{error:#}"),
                "could not inspect superseded host job groups");
        }
    }
    let stopping: Vec<&JobRecord> = interrupted.iter().chain(&retry).chain(&cancelled).collect();
    for record in stopping {
        kill_groups(work_dir, &record.stage_id, &record.id);
    }
    Ok(interrupted)
}

fn retire_historical_groups(work_dir: &Path, stage_id: &str, current: Option<&str>) -> Result<()> {
    for job_id in store::job_ids(work_dir, stage_id)? {
        if Some(job_id.as_str()) == current {
            continue;
        }
        match store::load_groups(work_dir, stage_id, &job_id) {
            Ok(groups) if !groups.is_empty() => {
                kill_groups(work_dir, stage_id, &job_id);
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(stage = %stage_id, job = %job_id,
                error = %format!("{error:#}"), "could not inspect old group controls"),
        }
    }
    Ok(())
}

fn interrupt(work_dir: &Path, stage_id: &str) -> Result<Option<Found>> {
    let _lock = store::lock_stage(work_dir, stage_id)?;
    let Some(mut record) = store::load_current(work_dir, stage_id)? else {
        return Ok(None);
    };
    if store::cancelled_after_start(&record) {
        return Ok(Some(Found::Cancelled(record)));
    }
    if matches!(
        record.state,
        JobState::Interrupted | JobState::Passed | JobState::Failed
    ) {
        return Ok(Some(Found::Retry(record)));
    }
    if record.state != JobState::Running {
        return Ok(None);
    }
    record.state = JobState::Interrupted;
    record.detail = Some(RESTART_DETAIL.to_string());
    record.finished_at = Some(Utc::now());
    store::save_current(work_dir, &record)?;
    Ok(Some(Found::Interrupted(record)))
}
