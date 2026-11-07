//! Durable application retries for terminal host jobs. The job directory owns
//! its attempt count and applied marker, so replacing `current.json` or
//! restarting the daemon cannot lose pending work or reset its retry cap.

use std::path::Path;

use anyhow::{ensure, Context, Result};

use super::apply::{apply_finished, APPLIED_MARKER};
use super::{bounded_detail, store, verdict_complete, JobKind, JobRecord};
use crate::fs::session_files::load_session_exact;
use crate::models::session::SessionType;
use crate::models::stage::StageStatus;
use crate::stage_service::ownership::owns_stage_as;
use crate::verify::contracts::refusal::{
    problem_lines, record_job_refusal, run_output_line, JobRefusalOrigin, INFRASTRUCTURE_PREFIX,
};
use crate::verify::contracts::store::load_freeze;
use crate::verify::transitions::load_stage;

const MAX_APPLY_ATTEMPTS: u32 = 10;
const ATTEMPTS_FILE: &str = "apply-attempts";

/// Every retained terminal job without an `applied` marker. Job copies
/// survive both a later submission and a daemon restart.
pub(crate) fn unapplied_terminal_records(work_dir: &Path) -> Result<Vec<JobRecord>> {
    let mut unapplied = Vec::new();
    for stage_id in store::stage_ids(work_dir)? {
        match unapplied_of(work_dir, &stage_id) {
            Ok(records) => unapplied.extend(records),
            Err(error) => tracing::warn!(stage_id = %stage_id,
                error = %format!("{error:#}"), "could not list unapplied host jobs for stage"),
        }
    }
    // Latest jobs publish first; the job-bound refusal writer then refuses
    // any older result for the same contract writer.
    unapplied.sort_by(|left, right| {
        left.stage_id
            .cmp(&right.stage_id)
            .then_with(|| right.submission_order.cmp(&left.submission_order))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(unapplied)
}

fn unapplied_of(work_dir: &Path, stage_id: &str) -> Result<Vec<JobRecord>> {
    let mut records = Vec::new();
    for job_id in store::job_ids(work_dir, stage_id)? {
        let found = (|| -> Result<Option<JobRecord>> {
            let Some(record) = store::load_job_record(work_dir, stage_id, &job_id)? else {
                anyhow::bail!("job record is missing");
            };
            ensure!(
                record.id == job_id && record.stage_id == stage_id,
                "job record identity does not match its directory"
            );
            if !record.state.is_terminal()
                || store::load_job_file(work_dir, stage_id, &job_id, APPLIED_MARKER)?.is_some()
            {
                return Ok(None);
            }
            Ok(Some(record))
        })();
        match found {
            Ok(Some(record)) => records.push(record),
            Ok(None) => {}
            Err(error) => tracing::warn!(stage_id = %stage_id, job = %job_id,
                error = %format!("{error:#}"), "could not inspect host job for application"),
        }
    }
    Ok(records)
}

pub(crate) fn apply_pending(work_dir: &Path) {
    match unapplied_terminal_records(work_dir) {
        Ok(records) => {
            for record in records {
                apply_record(work_dir, &record);
            }
        }
        Err(error) => tracing::warn!(error = %format!("{error:#}"),
            "could not list unapplied host jobs"),
    }
}

pub(crate) fn apply_record(work_dir: &Path, record: &JobRecord) {
    if let Err(error) = try_apply(work_dir, record) {
        tracing::warn!(stage = %record.stage_id, job = %record.id,
            error = %format!("{error:#}"), "host job application remains pending");
    }
}

fn try_apply(work_dir: &Path, record: &JobRecord) -> Result<()> {
    if store::load_job_file(work_dir, &record.stage_id, &record.id, APPLIED_MARKER)?.is_some() {
        return Ok(());
    }
    let attempts = load_attempts(work_dir, record)?;
    if attempts == MAX_APPLY_ATTEMPTS {
        return give_up(work_dir, record, "application retries were exhausted");
    }
    let next = attempts + 1;
    store::save_job_file(
        work_dir,
        &record.stage_id,
        &record.id,
        ATTEMPTS_FILE,
        next.to_string().as_bytes(),
    )?;
    match apply_finished(work_dir, record) {
        Ok(applied) => tracing::info!(stage = %record.stage_id, job = %record.id,
            applied = ?applied, "host job applied"),
        Err(error) if next < MAX_APPLY_ATTEMPTS => tracing::warn!(
            stage = %record.stage_id, job = %record.id, attempt = next,
            error = %format!("{error:#}"), "host job could not be applied; retrying on the next tick"),
        Err(error) => give_up(work_dir, record, &format!("{error:#}"))?,
    }
    Ok(())
}

fn load_attempts(work_dir: &Path, record: &JobRecord) -> Result<u32> {
    let Some(bytes) = store::load_job_file(work_dir, &record.stage_id, &record.id, ATTEMPTS_FILE)?
    else {
        return Ok(0);
    };
    let text = std::str::from_utf8(&bytes).context("invalid apply attempt count")?;
    let attempts: u32 = text.parse().context("invalid apply attempt count")?;
    ensure!(
        attempts <= MAX_APPLY_ATTEMPTS,
        "apply attempt count exceeds cap"
    );
    Ok(attempts)
}

fn give_up(work_dir: &Path, record: &JobRecord, reason: &str) -> Result<()> {
    tracing::error!(stage = %record.stage_id, job = %record.id, reason,
        "host job could not be applied after {MAX_APPLY_ATTEMPTS} attempts");
    if record.kind == JobKind::Freeze {
        record_freeze_failure(work_dir, record, reason)?;
    }
    if record.kind == JobKind::VerdictComplete {
        verdict_complete::give_up(work_dir, record, reason)?;
    }
    let marker = bounded_detail(&format!("apply failed: {}", run_output_line(reason)));
    store::save_job_file(
        work_dir,
        &record.stage_id,
        &record.id,
        APPLIED_MARKER,
        marker.as_bytes(),
    )
}

pub(super) fn record_freeze_failure(
    work_dir: &Path,
    record: &JobRecord,
    reason: &str,
) -> Result<()> {
    let stage = load_stage(&record.stage_id, work_dir)?;
    let session = load_session_exact(work_dir, &record.session_id)?;
    let owns = session.is_some_and(|session| {
        owns_stage_as(
            &stage,
            &session,
            &record.stage_id,
            &record.session_id,
            &[SessionType::Contract],
        )
    });
    let running = matches!(
        stage.status,
        StageStatus::Executing | StageStatus::WaitingForInput
    );
    if !owns || !running || load_freeze(work_dir, &record.stage_id)?.is_some() {
        return Ok(());
    }
    let detail = format!(
        "{INFRASTRUCTURE_PREFIX}the contract run could not be applied: {}",
        run_output_line(reason)
    );
    record_job_refusal(
        work_dir,
        &record.stage_id,
        &record.session_id,
        JobRefusalOrigin {
            job_id: record.id.clone(),
            submission_order: record.submission_order,
        },
        &problem_lines(&detail),
    )?;
    Ok(())
}
