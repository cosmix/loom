//! Queueing, joining, waiting and cancelling a stage's host jobs.
//!
//! The current slot controls the runner; uncovered checks of its session and
//! tree share a waiting slot. Cancellation writes state, and the runner kills
//! a child whose record is no longer `Running`. Every read-modify-write happens
//! under `store::lock_stage`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::models::dispute::FileReplacement;
use crate::native_worker::index::stage_disposition;

use super::{bounded_detail, new_job_id, pending, store, JobKind, JobRecord, JobState};

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
    /// The 0-based criteria a check runs; `None` runs every one. Only a
    /// check carries one.
    pub criteria: Option<Vec<usize>>,
}

/// What `submit` did with a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// A new job was queued.
    Queued(JobRecord),
    /// The request joined the stage's running or queued job.
    Joined(JobRecord),
    /// A check waits behind the live check of the same session and tree.
    Waiting {
        pending: Box<JobRecord>,
        behind: String,
    },
    /// The stage's job was cancelled for a new one.
    Superseded {
        cancelled: Box<JobRecord>,
        queued: Box<JobRecord>,
    },
    /// The request was refused; the reason is for the requesting session.
    Refused(String),
}

/// The job file of a trial's replacement files, a JSON `[FileReplacement]` laid over frozen files.
pub(crate) const REPLACEMENTS_FILE: &str = "replacements.json";

/// Queue, join, wait behind a check of the same session and tree, or supersede.
/// A check never cancels a completion or contract run (common.md 3.3).
pub fn submit(
    work_dir: &Path,
    request: &SubmitRequest<'_>,
    now: DateTime<Utc>,
) -> Result<Submission> {
    submit_selection(work_dir, request, Selection::Criteria, now)
}

/// [`submit`] a check of the frozen contracts (`--contracts`); `replacements` make it a trial of
/// those files. Its own selection, it neither joins nor is joined by another job, nor waits.
pub fn submit_contracts(
    work_dir: &Path,
    request: &SubmitRequest<'_>,
    replacements: &[FileReplacement],
    now: DateTime<Utc>,
) -> Result<Submission> {
    submit_selection(work_dir, request, Selection::Contracts(replacements), now)
}

/// [`submit`] a check of the request's criteria at the stage's base commit
/// (`loom check --at-base`), its own selection as a contracts check is.
pub fn submit_at_base(
    work_dir: &Path,
    request: &SubmitRequest<'_>,
    now: DateTime<Utc>,
) -> Result<Submission> {
    submit_selection(work_dir, request, Selection::AtBase, now)
}

/// What a job runs: the stage's criteria, its frozen contracts with a trial's
/// replacement files (none for a plain check), or criteria at the base.
#[derive(Clone, Copy)]
enum Selection<'a> {
    Criteria,
    Contracts(&'a [FileReplacement]),
    AtBase,
}

fn submit_selection(
    work_dir: &Path,
    request: &SubmitRequest<'_>,
    selection: Selection<'_>,
    now: DateTime<Utc>,
) -> Result<Submission> {
    let own = !matches!(selection, Selection::Criteria);
    if let Some(problem) = invalid_binding(request, selection) {
        return Ok(Submission::Refused(problem.to_string()));
    }
    if let Some(blocker) = stage_disposition(work_dir, request.stage_id).blocker(request.stage_id) {
        return Ok(Submission::Refused(blocker));
    }
    let lock = store::lock_stage(work_dir, request.stage_id)?;
    let Some(mut current) = live_current(&lock, work_dir, request.stage_id)? else {
        return queue(work_dir, request, selection, now).map(Submission::Queued);
    };
    if covers(&current, request, own) {
        add_request(&mut current, request.request_id);
        store::save_current(work_dir, &current)?;
        return Ok(Submission::Joined(current));
    }
    if request.kind == JobKind::Check && current.kind != JobKind::Check {
        return Ok(Submission::Refused(CHECK_BEHIND_RUN.to_string()));
    }
    if !own && waits_behind(&current, request) {
        return wait_behind(&lock, work_dir, &current, request, now);
    }
    let queued = new_record(work_dir, request, selection, now)?;
    let reason = format!("superseded by {}", queued.id);
    pending::cancel_locked(&lock, work_dir, request.stage_id, &reason, now)?;
    end(&mut current, &reason, now);
    store::save_current(work_dir, &current)?;
    save_replacements(work_dir, &queued, selection)?;
    persist_queued(work_dir, &queued)?;
    Ok(Submission::Superseded {
        cancelled: Box::new(current),
        queued: Box::new(queued),
    })
}

/// Whether `text` is exactly one of `lengths` long and all lowercase hex.
pub(crate) fn is_lower_hex(text: &str, lengths: &[usize]) -> bool {
    lengths.contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Why the request's tree binding is malformed, if it is. Every route that
/// queues a job passes through `submit`, so none can store an unverifiable
/// binding.
fn invalid_binding(request: &SubmitRequest<'_>, selection: Selection<'_>) -> Option<&'static str> {
    if !is_lower_hex(request.head, &[40, 64]) {
        return Some("head must be 40 or 64 lowercase hex characters");
    }
    if !is_lower_hex(request.tree_digest, &[64]) {
        return Some("tree_digest must be 64 lowercase hex characters");
    }
    let contracts = matches!(selection, Selection::Contracts(_));
    if contracts && request.kind != JobKind::Check {
        return Some("only a check runs the contracts alone");
    }
    match &request.criteria {
        None if matches!(selection, Selection::AtBase) => Some("an at-base check names criteria"),
        Some(_) if contracts => Some("a contracts check runs no acceptance criteria"),
        Some(_) if request.kind != JobKind::Check => Some("only a check runs a criterion subset"),
        Some(criteria) if criteria.is_empty() => Some("criteria must name at least one criterion"),
        _ => None,
    }
}

/// Cancel the waiting job first, then current. Return the cancelled current,
/// else the cancelled waiting job; no live job returns `None`.
pub fn cancel(work_dir: &Path, stage_id: &str, reason: &str) -> Result<Option<JobRecord>> {
    // Most callers (every takedown, reset and retry) find no job; looking
    // first keeps them from creating the stage's directory to lock it.
    let live = |record: &JobRecord| !record.state.is_terminal();
    // Read pending BEFORE current: promotion saves the waiting job as current
    // and then clears pending, so this order always sees a job moving between
    // the slots, while the reverse order can miss it and leave it live.
    if store::load_pending(work_dir, stage_id)?.is_none()
        && !store::load_current(work_dir, stage_id)?.is_some_and(|record| live(&record))
    {
        return Ok(None);
    }
    let lock = store::lock_stage(work_dir, stage_id)?;
    let waiting = pending::cancel_locked(&lock, work_dir, stage_id, reason, Utc::now())?;
    let Some(mut record) = store::load_current(work_dir, stage_id)?.filter(live) else {
        return Ok(waiting);
    };
    end(&mut record, reason, Utc::now());
    store::save_current(work_dir, &record)?;
    Ok(Some(record))
}

/// Cancel the observed job in either slot, plus its waiting check when current
/// matches. A replacement must not be retired by stale cleanup.
pub fn cancel_if_id(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    reason: &str,
) -> Result<Option<JobRecord>> {
    let lock = store::lock_stage(work_dir, stage_id)?;
    let Some(mut record) = store::load_current(work_dir, stage_id)?
        .filter(|record| record.id == job_id && !record.state.is_terminal())
    else {
        if pending::live_locked(&lock, work_dir, stage_id)?
            .is_some_and(|record| record.id == job_id)
        {
            return pending::cancel_locked(&lock, work_dir, stage_id, reason, Utc::now());
        }
        return Ok(None);
    };
    pending::cancel_locked(&lock, work_dir, stage_id, reason, Utc::now())?;
    end(&mut record, reason, Utc::now());
    store::save_current(work_dir, &record)?;
    Ok(Some(record))
}

/// Whether the non-terminal `current` job answers `request` as well: the same tree and kind, a
/// cached request is answered by an uncached run too, and the job runs every criterion the request
/// selects. A contracts check or a check at the base (`own`, or the current job's) answers nothing
/// else and is answered by nothing: a trial's replacement bytes are its own selection.
fn covers(current: &JobRecord, request: &SubmitRequest<'_>, own: bool) -> bool {
    !own && !current.contracts
        && !current.at_base
        && current.head == request.head
        && current.tree_digest == request.tree_digest
        && (current.no_cache || !request.no_cache)
        && runs_selection(current.criteria.as_deref(), request.criteria.as_deref())
        && match (current.kind, request.kind) {
            (JobKind::Complete, JobKind::Complete) => current.session_id == request.session_id,
            (JobKind::Complete, JobKind::Check) => true,
            (current_kind, request_kind) => current_kind == request_kind,
        }
}

/// Whether a job running `job`'s criteria runs all of `request`'s: a job
/// without a selection runs every criterion, and only such a job answers a
/// request for every criterion.
fn runs_selection(job: Option<&[usize]>, request: Option<&[usize]>) -> bool {
    match (job, request) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(job), Some(request)) => request.iter().all(|index| job.contains(index)),
    }
}

fn new_record(
    work_dir: &Path,
    request: &SubmitRequest<'_>,
    selection: Selection<'_>,
    now: DateTime<Utc>,
) -> Result<JobRecord> {
    Ok(JobRecord {
        id: new_job_id(),
        submission_order: store::next_submission_order(work_dir, request.stage_id)?,
        stage_id: request.stage_id.to_string(),
        session_id: request.session_id.to_string(),
        kind: request.kind,
        head: request.head.to_string(),
        tree_digest: request.tree_digest.to_string(),
        no_cache: request.no_cache,
        criteria: request.criteria.clone(),
        contracts: matches!(selection, Selection::Contracts(_)),
        at_base: matches!(selection, Selection::AtBase),
        state: JobState::Queued,
        requests: vec![request.request_id.to_string()],
        queued_at: now,
        started_at: None,
        finished_at: None,
        pid: None,
        detail: None,
    })
}

fn queue(
    work_dir: &Path,
    request: &SubmitRequest<'_>,
    selection: Selection<'_>,
    now: DateTime<Utc>,
) -> Result<JobRecord> {
    let queued = new_record(work_dir, request, selection, now)?;
    save_replacements(work_dir, &queued, selection)?;
    persist_queued(work_dir, &queued)?;
    Ok(queued)
}

/// Save a trial's replacement files as `queued`'s job file before the job
/// becomes current, so the runner never starts it without them.
fn save_replacements(work_dir: &Path, queued: &JobRecord, selection: Selection<'_>) -> Result<()> {
    let replacements = match selection {
        Selection::Contracts(files) if !files.is_empty() => files,
        _ => return Ok(()),
    };
    let bytes = serde_json::to_vec(replacements)?;
    store::save_job_file(
        work_dir,
        &queued.stage_id,
        &queued.id,
        REPLACEMENTS_FILE,
        &bytes,
    )
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

pub(super) fn end(record: &mut JobRecord, detail: &str, now: DateTime<Utc>) {
    record.state = JobState::Cancelled;
    record.detail = Some(bounded_detail(detail));
    record.finished_at = Some(now);
}

fn live_current(
    lock: &store::StageLock,
    work_dir: &Path,
    stage_id: &str,
) -> Result<Option<JobRecord>> {
    // An interrupted promotion can leave both slots naming current. Clear
    // that pointer before a new request can widen it as a waiting selection.
    pending::promote_locked(lock, work_dir, stage_id)?;
    Ok(store::load_current(work_dir, stage_id)?.filter(|job| !job.state.is_terminal()))
}

/// Same session only: the cache policy follows the job's session.
fn waits_behind(current: &JobRecord, request: &SubmitRequest<'_>) -> bool {
    request.kind == JobKind::Check
        && current.kind == JobKind::Check
        && current.head == request.head
        && current.tree_digest == request.tree_digest
        && current.session_id == request.session_id
}

fn wait_behind(
    lock: &store::StageLock,
    work_dir: &Path,
    current: &JobRecord,
    request: &SubmitRequest<'_>,
    now: DateTime<Utc>,
) -> Result<Submission> {
    let waiting = match pending::live_locked(lock, work_dir, request.stage_id)? {
        Some(mut waiting) => {
            widen(&mut waiting, request);
            waiting
        }
        None => new_record(work_dir, request, Selection::Criteria, now)?,
    };
    store::save_pending(work_dir, &waiting)?;
    Ok(Submission::Waiting {
        pending: Box::new(waiting),
        behind: current.id.clone(),
    })
}

fn widen(record: &mut JobRecord, request: &SubmitRequest<'_>) {
    record.criteria = union(record.criteria.as_deref(), request.criteria.as_deref());
    record.no_cache |= request.no_cache;
    add_request(record, request.request_id);
}

fn union(a: Option<&[usize]>, b: Option<&[usize]>) -> Option<Vec<usize>> {
    Some(
        a?.iter()
            .chain(b?)
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    )
}

fn add_request(record: &mut JobRecord, request_id: &str) {
    if !record.requests.iter().any(|id| id == request_id) {
        record.requests.push(request_id.to_string());
    }
    let excess = record.requests.len().saturating_sub(MAX_REQUESTS);
    record.requests = record.requests.split_off(excess);
}

#[cfg(test)]
#[path = "submit_native_hold_tests.rs"]
pub(crate) mod native_hold_tests;
