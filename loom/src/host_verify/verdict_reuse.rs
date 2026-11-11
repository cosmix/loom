//! A verdict completion without a run. A verdict that changes nothing in the
//! tree (accepted test-integrity events, an accepted dispute of a criterion)
//! leaves the stage's committed head and check definition where its latest
//! `loom stage complete` run found them, so when that run passed, a second
//! run of the same criteria on the same tree only repeats it. The daemon then
//! completes the held stage from the stored result, binding it exactly as the
//! application of a verdict completion run does (`verify_bound_run`: the run
//! passed at the head it was asked for, the stage's committed content is that
//! head, and the check definition is the one the run was bound to), after the
//! completion gates have passed on the stored state. [`latest_of`] finds the
//! stage's runs on record for both this and the submission of a new run.

use std::path::Path;

use anyhow::Result;

use super::verdict_complete::{complete_held, VerdictStart};
use super::{store, JobKind, JobRecord, JobResult, JobState};

/// Whether `latest`, the stage's newest `loom stage complete` run, decides
/// the held stage by itself: it passed, its result verified the head it was
/// asked for, and no verdict completion run was submitted after it (a later
/// run of the same tree that did not pass outranks it). The caller has bound
/// it to the stage's committed head and current check definition.
pub(super) fn decides(work_dir: &Path, latest: &JobRecord, result: &JobResult) -> Result<bool> {
    if latest.state != JobState::Passed || !result.passed || result.head != latest.head {
        return Ok(false);
    }
    let kinds = [JobKind::Complete, JobKind::VerdictComplete];
    let newest = latest_of(work_dir, &latest.stage_id, &kinds)?;
    Ok(newest.is_some_and(|newest| newest.id == latest.id))
}

/// Complete the held stage from `latest`'s passing `result`. A binding the
/// stored run no longer satisfies leaves the stage to a fresh session; one
/// whose stage left its hold meanwhile hands nothing back
/// (`hand_back_held_stage` refuses a stage that is not held).
pub(super) fn complete_from_run(
    work_dir: &Path,
    latest: &JobRecord,
    result: &JobResult,
) -> Result<VerdictStart> {
    Ok(match complete_held(work_dir, latest, result)? {
        Ok(()) => VerdictStart::Reused {
            job: latest.id.clone(),
            commit: result.head.clone(),
        },
        Err(reason) => VerdictStart::Ineligible(reason),
    })
}

/// The stage's newest record of one of `kinds` by `(submission_order,
/// queued_at)`, whatever its state. With `Complete` alone it is the agent's
/// own latest `loom stage complete`. A record that cannot be read is skipped.
/// `submit` prunes old job directories, so read this before it.
pub(super) fn latest_of(
    work_dir: &Path,
    stage_id: &str,
    kinds: &[JobKind],
) -> Result<Option<JobRecord>> {
    if !store::stage_dir(work_dir, stage_id)?.is_dir() {
        return Ok(None);
    }
    let latest = store::job_ids(work_dir, stage_id)?
        .into_iter()
        .filter_map(|job_id| readable_record(work_dir, stage_id, &job_id))
        .filter(|record| kinds.contains(&record.kind))
        .max_by_key(|record| (record.submission_order, record.queued_at));
    Ok(latest)
}

fn readable_record(work_dir: &Path, stage_id: &str, job_id: &str) -> Option<JobRecord> {
    match store::load_job_record(work_dir, stage_id, job_id) {
        Ok(Some(record)) if record.id == job_id && record.stage_id == stage_id => Some(record),
        Ok(Some(_)) => {
            tracing::warn!(stage = %stage_id, job = %job_id,
                "host job record identity does not match its directory; skipped");
            None
        }
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(stage = %stage_id, job = %job_id, error = %format!("{error:#}"),
                "could not read a host job record; skipped");
            None
        }
    }
}

#[cfg(test)]
#[path = "verdict_reuse_tests.rs"]
mod tests;
