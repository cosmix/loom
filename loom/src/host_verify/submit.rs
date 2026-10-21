//! Queueing, joining and cancelling a stage's host job.
//!
//! The stage's current record is the single source of truth: whoever cancels
//! only writes its state, and the runner kills a child whose record is no
//! longer `Running` on its next tick. Every read-modify-write here happens
//! under `store::lock_stage`.

use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::{bounded_detail, new_job_id, store, JobKind, JobRecord, JobState};

/// Relay request ids a record keeps; the oldest goes first.
const MAX_REQUESTS: usize = 16;

/// Why a check waits behind a completion or contract run of its stage.
const CHECK_BEHIND_RUN: &str =
    "a completion or contract run is in progress for this stage; run the check again when it ends";

/// One relayed request for a host job.
pub struct SubmitRequest<'a> {
    pub stage_id: &'a str,
    pub session_id: &'a str,
    pub request_id: &'a str,
    pub kind: JobKind,
    pub head: &'a str,
    pub tree_digest: &'a str,
    pub no_cache: bool,
}

/// What `submit` did with a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// A new job was queued.
    Queued(JobRecord),
    /// The request joined the stage's running or queued job.
    Joined(JobRecord),
    /// The stage's job was cancelled for a new one.
    Superseded {
        cancelled: Box<JobRecord>,
        queued: Box<JobRecord>,
    },
    /// The request was refused; the reason is for the requesting session.
    Refused(String),
}

/// Queue `request` as the stage's job, join it to the job that already
/// covers it, or supersede a job that does not (common.md 3.3). A check never
/// cancels a completion or contract run.
pub fn submit(
    work_dir: &Path,
    request: &SubmitRequest<'_>,
    now: DateTime<Utc>,
) -> Result<Submission> {
    let _lock = store::lock_stage(work_dir, request.stage_id)?;
    let current = store::load_current(work_dir, request.stage_id)?
        .filter(|record| !record.state.is_terminal());
    let Some(mut current) = current else {
        return queue(work_dir, request, now).map(Submission::Queued);
    };
    if covers(&current, request) {
        if !current.requests.iter().any(|id| id == request.request_id) {
            current.requests.push(request.request_id.to_string());
        }
        let excess = current.requests.len().saturating_sub(MAX_REQUESTS);
        current.requests = current.requests.split_off(excess);
        store::save_current(work_dir, &current)?;
        return Ok(Submission::Joined(current));
    }
    if request.kind == JobKind::Check && current.kind != JobKind::Check {
        return Ok(Submission::Refused(CHECK_BEHIND_RUN.to_string()));
    }
    let queued = new_record(request, now);
    end(&mut current, &format!("superseded by {}", queued.id), now);
    store::save_current(work_dir, &current)?;
    persist_queued(work_dir, &queued)?;
    Ok(Submission::Superseded {
        cancelled: Box::new(current),
        queued: Box::new(queued),
    })
}

/// Cancel the stage's non-terminal job with `reason` and return it; a
/// terminal or missing record is `None`.
pub fn cancel(work_dir: &Path, stage_id: &str, reason: &str) -> Result<Option<JobRecord>> {
    // Most callers (every takedown, reset and retry) find no job; looking
    // first keeps them from creating the stage's directory to lock it.
    let live = |record: &JobRecord| !record.state.is_terminal();
    if !store::load_current(work_dir, stage_id)?.is_some_and(|record| live(&record)) {
        return Ok(None);
    }
    let _lock = store::lock_stage(work_dir, stage_id)?;
    let Some(mut record) = store::load_current(work_dir, stage_id)?.filter(live) else {
        return Ok(None);
    };
    end(&mut record, reason, Utc::now());
    store::save_current(work_dir, &record)?;
    Ok(Some(record))
}

/// Whether the non-terminal `current` job answers `request` as well: the same
/// tree and kind, and a cached request is answered by an uncached run too.
fn covers(current: &JobRecord, request: &SubmitRequest<'_>) -> bool {
    current.head == request.head
        && current.tree_digest == request.tree_digest
        && (current.no_cache || !request.no_cache)
        && current.kind == request.kind
}

fn new_record(request: &SubmitRequest<'_>, now: DateTime<Utc>) -> JobRecord {
    JobRecord {
        id: new_job_id(),
        stage_id: request.stage_id.to_string(),
        session_id: request.session_id.to_string(),
        kind: request.kind,
        head: request.head.to_string(),
        tree_digest: request.tree_digest.to_string(),
        no_cache: request.no_cache,
        state: JobState::Queued,
        requests: vec![request.request_id.to_string()],
        queued_at: now,
        started_at: None,
        finished_at: None,
        pid: None,
        detail: None,
    }
}

fn queue(work_dir: &Path, request: &SubmitRequest<'_>, now: DateTime<Utc>) -> Result<JobRecord> {
    let queued = new_record(request, now);
    persist_queued(work_dir, &queued)?;
    Ok(queued)
}

/// Make `queued` the stage's current job, then prune old job directories. A
/// failed prune costs disk space only and never refuses the job.
fn persist_queued(work_dir: &Path, queued: &JobRecord) -> Result<()> {
    store::save_current(work_dir, queued)?;
    if let Err(error) = store::prune_jobs(work_dir, &queued.stage_id, &queued.id) {
        tracing::warn!(
            stage = %queued.stage_id,
            error = %format!("{error:#}"),
            "could not prune old host job directories",
        );
    }
    Ok(())
}

fn end(record: &mut JobRecord, detail: &str, now: DateTime<Utc>) {
    record.state = JobState::Cancelled;
    record.detail = Some(bounded_detail(detail));
    record.finished_at = Some(now);
}
