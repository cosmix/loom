//! Progress evidence for a Codex companion job the ledger still calls running.
//!
//! A companion job that dies without writing a terminal record leaves its
//! record frozen at `status: running` forever, and a bounded wait bound to it
//! sits until its own deadline with nothing to report. Two observable facts
//! settle that case without trusting the record's own status: whether the
//! worker process still exists, and whether anything the job writes has
//! changed recently.
//!
//! Neither fact is consulted for a job the ledger already calls terminal --
//! that outcome is authoritative and this module defers to it.

use std::path::Path;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};

use crate::models::forward_receipt::job_record::CompanionJob;
use crate::subagent_lifecycle::CodexEvidenceOutcome;

use super::jobs::classify_job;

/// What direct observation says about a job the ledger calls running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompanionProgress {
    /// Making progress, or not observable as anything else.
    Running,
    /// The worker process is gone, so the job can never reach a terminal
    /// record on its own.
    Dead(String),
    /// The process is alive but nothing it writes has changed within budget.
    Stalled(String),
}

/// Classify a companion job's progress as of `now`.
///
/// Returns [`CompanionProgress::Running`] unchanged for any job that is not
/// currently running, including one whose record cannot be classified at all:
/// the lifecycle ledger owns every terminal verdict and this check must never
/// contradict it.
pub(crate) fn companion_progress(
    job: &CompanionJob,
    budget: Duration,
    now: SystemTime,
) -> CompanionProgress {
    if !matches!(
        classify_job(job).map(|observation| observation.outcome),
        Ok(CodexEvidenceOutcome::Running)
    ) {
        return CompanionProgress::Running;
    }
    // A queued job has not been given a worker process yet, so an absent pid is
    // normal rather than evidence of death, and it may legitimately sit behind
    // the companion's concurrency cap for longer than the stall budget.
    if job.status.as_deref() == Some("queued") {
        return CompanionProgress::Running;
    }
    if let Some(pid) = job.pid {
        if !crate::process::is_process_alive(pid) {
            return CompanionProgress::Dead(format!(
                "companion job {} process {pid} is gone while its record says {}/{}",
                job.id,
                job.status.as_deref().unwrap_or("no status"),
                job.phase.as_deref().unwrap_or("no phase"),
            ));
        }
    }
    match stale_for(job, now) {
        Some(idle) if idle > budget => CompanionProgress::Stalled(format!(
            "companion job {} shows no progress for {}s (stall budget {}s); log {}",
            job.id,
            idle.as_secs(),
            budget.as_secs(),
            job.log_file
                .as_deref()
                .map_or_else(|| "unknown".to_owned(), |path| path.display().to_string()),
        )),
        _ => CompanionProgress::Running,
    }
}

/// How long since the newest thing this job is known to have written.
///
/// `None` means the record carries no usable freshness signal at all, which is
/// not evidence of a stall.
fn stale_for(job: &CompanionJob, now: SystemTime) -> Option<Duration> {
    let freshest = [
        log_modified(job.log_file.as_deref()),
        job.updated_at.and_then(as_system_time),
        job.started_at.and_then(as_system_time),
        job.created_at.and_then(as_system_time),
    ]
    .into_iter()
    .flatten()
    .max()?;
    // A record stamped in the future (clock skew) reads as fresh, not stale.
    Some(now.duration_since(freshest).unwrap_or_default())
}

fn log_modified(path: Option<&Path>) -> Option<SystemTime> {
    std::fs::metadata(path?).ok()?.modified().ok()
}

fn as_system_time(value: DateTime<Utc>) -> Option<SystemTime> {
    let seconds = u64::try_from(value.timestamp()).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
