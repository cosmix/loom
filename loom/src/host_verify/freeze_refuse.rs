//! How a finished freeze job that did not freeze words and records its
//! refusal.
//!
//! A static refusal (the job failed only at the format gate) and a run that
//! was interrupted or stopped by the daemon are returned to the waiting
//! writer, which fixes its files or reruns the freeze
//! (`refusal::record_static_refusal`); the third of a session, and every other
//! refusal, is recorded for the tick to file as a `freeze-refusal` dispute.

use std::path::Path;

use anyhow::Result;

use super::apply::Applied;
use super::{store, JobRecord};
use crate::models::dispute::truncate_to_byte_limit;
use crate::verify::contracts::refusal::{
    problem_lines, record_job_refusal, record_static_refusal, run_output_line, JobRefusalOrigin,
    StaticRefusal, INFRASTRUCTURE_PREFIX, STATIC_RETURN_LIMIT,
};

/// Most bytes of the refusal a failed run records.
pub(super) const REFUSAL_MAX_BYTES: usize = 4 * 1024;

/// The label of the freeze job's static gate outcome.
const FORMAT_LABEL: &str = "format";

/// Starts the message of a refusal recorded for the tick to file.
const FILED_PREFIX: &str = "filed as a freeze-refusal dispute: ";

/// Each failing outcome's label and summary, loom's own judgement, then the
/// run's error, else the job's detail. The error and the detail are quoted as
/// one run-output line each ([`run_output_line`]): they can carry the child's
/// log and the contract run's output, which the writer's tests print. A result
/// that cannot be read counts as none, so the refusal still carries the
/// detail.
pub(super) fn failure_message(work_dir: &Path, record: &JobRecord) -> String {
    let result = load_result(work_dir, record);
    let mut lines: Vec<String> = result
        .iter()
        .flat_map(|result| &result.criteria)
        .filter(|outcome| !outcome.passed)
        .map(|outcome| format!("{}: {}", outcome.label, outcome.summary))
        .collect();
    lines.extend(
        result
            .and_then(|result| result.error)
            .map(|error| run_output_line(&error)),
    );
    if lines.is_empty() {
        let line = match record.detail.as_deref() {
            Some(detail) => run_output_line(detail),
            None => "the contract run failed".to_string(),
        };
        lines.push(line);
    }
    lines.join("\n")
}

/// The job's saved result; one that cannot be read counts as none.
fn load_result(work_dir: &Path, record: &JobRecord) -> Option<super::JobResult> {
    store::load_result(work_dir, &record.stage_id, &record.id).unwrap_or_else(|error| {
        tracing::warn!(
            job = %record.id,
            error = %format!("{error:#}"),
            "cannot read a freeze job's result",
        );
        None
    })
}

/// Whether the failed job `record` is a static failure: its saved result has
/// no error, its `format` outcome failed and every other outcome passed (a
/// `contract:<id>` outcome passes when the contract was red).
pub(super) fn static_failure(work_dir: &Path, record: &JobRecord) -> bool {
    let Some(result) = load_result(work_dir, record) else {
        return false;
    };
    result.error.is_none()
        && result
            .criteria
            .iter()
            .any(|outcome| outcome.label == FORMAT_LABEL && !outcome.passed)
        && result
            .criteria
            .iter()
            .all(|outcome| outcome.passed || outcome.label == FORMAT_LABEL)
}

/// Refuse the failed job `record`: a static failure is returned to the
/// writer while its session is below the limit, anything else is filed.
pub(super) fn refuse_failed(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    let message = failure_message(work_dir, record);
    if static_failure(work_dir, record) {
        return return_to_writer(work_dir, record, &message);
    }
    refuse(work_dir, record, &message)
}

/// Return an interrupted or daemon-stopped run to the waiting writer, which
/// is told to rerun the freeze; it shares the session's count and limit with
/// the static refusals, and its first problem is [`INFRASTRUCTURE_PREFIX`]ed
/// so a filed one still tells the adjudicator to have the freeze rerun.
pub(super) fn return_infrastructure(
    work_dir: &Path,
    record: &JobRecord,
    message: &str,
) -> Result<Applied> {
    let message = format!("{INFRASTRUCTURE_PREFIX}{message}");
    return_to_writer(work_dir, record, &message)
}

fn return_to_writer(work_dir: &Path, record: &JobRecord, message: &str) -> Result<Applied> {
    let message = truncate_to_byte_limit(message, REFUSAL_MAX_BYTES);
    let recorded = record_static_refusal(
        work_dir,
        &record.stage_id,
        &record.session_id,
        origin(record),
        &problem_lines(&message),
    )?;
    Ok(match recorded {
        StaticRefusal::Returned { count } => Applied::FreezeRefused(format!(
            "returned to the contract writer ({count} of {STATIC_RETURN_LIMIT}): fix what the \
             output above names and run loom stage contracts freeze {} again\n{message}",
            record.stage_id
        )),
        StaticRefusal::Filed => Applied::FreezeRefused(format!("{FILED_PREFIX}{message}")),
        StaticRefusal::Superseded => superseded(),
    })
}

/// Record the refusal `message`, cut to 4 KiB, for the job's session: one
/// problem per line. The tick step files it while that session is still the
/// stage's contract writer, and drops it otherwise.
pub(super) fn refuse(work_dir: &Path, record: &JobRecord, message: &str) -> Result<Applied> {
    let message = truncate_to_byte_limit(message, REFUSAL_MAX_BYTES);
    let recorded = record_job_refusal(
        work_dir,
        &record.stage_id,
        &record.session_id,
        origin(record),
        &problem_lines(&message),
    )?;
    if recorded {
        Ok(Applied::FreezeRefused(format!("{FILED_PREFIX}{message}")))
    } else {
        Ok(superseded())
    }
}

/// [`refuse`] a run that failed through no one's change.
pub(super) fn refuse_infrastructure(
    work_dir: &Path,
    record: &JobRecord,
    message: &str,
) -> Result<Applied> {
    refuse(
        work_dir,
        record,
        &format!("{INFRASTRUCTURE_PREFIX}{message}"),
    )
}

fn origin(record: &JobRecord) -> JobRefusalOrigin {
    JobRefusalOrigin {
        job_id: record.id.clone(),
        submission_order: record.submission_order,
    }
}

fn superseded() -> Applied {
    Applied::Ignored("a newer freeze refusal is recorded".to_string())
}
