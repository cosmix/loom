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
//! stage's runs on record for both this and the submission of a new run, and
//! refuses to name one when any record of the history cannot be read.

use std::path::Path;

use anyhow::Result;

use super::verdict_complete::{complete_held, VerdictStart};
use super::{store, JobKind, JobRecord, JobResult, JobState};
use crate::handoff::completion::policy::effective_policy_hash;
use crate::models::stage::Stage;
use crate::verify::completion::IMPACT_TESTS_LABEL;

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
    // History that cannot be ordered queues a fresh run instead.
    let newest = latest_of(work_dir, &latest.stage_id, &kinds)?;
    Ok(matches!(newest, Ok(Some(newest)) if newest.id == latest.id))
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
/// own latest `loom stage complete`. History that cannot be ordered (a job
/// record that cannot be read, whose kind is then unknown, one that does not
/// match its directory, or an unreadable current record) is `Ok(Err(reason))`:
/// no older record may stand for the newest. The current record counts even
/// when its job directory is gone. `submit` prunes old job directories, so
/// read this before it.
pub(super) fn latest_of(
    work_dir: &Path,
    stage_id: &str,
    kinds: &[JobKind],
) -> Result<std::result::Result<Option<JobRecord>, String>> {
    if !store::stage_dir(work_dir, stage_id)?.is_dir() {
        return Ok(Ok(None));
    }
    let mut candidates = Vec::new();
    for job_id in store::job_ids(work_dir, stage_id)? {
        match store::load_job_record(work_dir, stage_id, &job_id) {
            Ok(Some(record)) if record.id == job_id && record.stage_id == stage_id => {
                if kinds.contains(&record.kind) {
                    candidates.push(record);
                }
            }
            Ok(Some(_)) => {
                return Ok(Err(format!(
                    "{UNORDERED}: job {job_id} does not match its directory"
                )));
            }
            Ok(None) => {}
            Err(error) => {
                return Ok(Err(format!(
                    "{UNORDERED}: job {job_id} is unreadable ({error:#})"
                )));
            }
        }
    }
    let current = match store::load_current(work_dir, stage_id) {
        Ok(current) => current.filter(|record| kinds.contains(&record.kind)),
        Err(error) => {
            return Ok(Err(format!(
                "{UNORDERED}: the current job record is unreadable ({error:#})"
            )));
        }
    };
    Ok(Ok(newest(candidates, current)))
}

const UNORDERED: &str = "the stage's host job history cannot be ordered";

/// The result of the latest completion run `record` when it can stand for a
/// completion without a session, else why it cannot: it left a result bound
/// to the stage's current check definition, and it passed or failed only its
/// impact tests. A sibling verdict that amended criteria, re-froze contracts
/// or changed the plan moves the policy, and the stage then needs a session
/// that reads the new one.
pub(super) fn counted_result(
    work_dir: &Path,
    stage: &Stage,
    record: &JobRecord,
) -> Result<std::result::Result<JobResult, &'static str>> {
    let Some(result) = store::load_result(work_dir, &stage.id, &record.id)? else {
        return Ok(Err(
            "the stage's latest `loom stage complete` run left no result",
        ));
    };
    if !run_counts(record.state, &result) {
        return Ok(Err(
            "the stage's latest `loom stage complete` run failed more than its impact tests",
        ));
    }
    let policy = effective_policy_hash(stage, work_dir)?;
    if result.policy_hash.as_deref() != Some(policy.as_str()) {
        return Ok(Err(
            "the stage's check definition changed since its latest `loom stage complete` run",
        ));
    }
    Ok(Ok(result))
}

/// Impact tests run in the child, so the run an accepted impact-tests dispute
/// follows always failed; the rerun counts the accepted failure as a note.
/// Any other failed check, or a pipeline error, needs the agent.
pub(super) fn run_counts(state: JobState, result: &JobResult) -> bool {
    let failed: Vec<&str> = result
        .criteria
        .iter()
        .filter(|outcome| !outcome.passed)
        .map(|outcome| outcome.label.as_str())
        .collect();
    match state {
        JobState::Passed => true,
        JobState::Failed => {
            result.error.is_none()
                && !failed.is_empty()
                && failed.iter().all(|label| *label == IMPACT_TESTS_LABEL)
        }
        _ => false,
    }
}

/// The newest of the per-job `candidates` and the `current` record. The
/// current record wins a tie with its own per-job copy, and wins outright
/// when that copy is gone.
fn newest(candidates: Vec<JobRecord>, current: Option<JobRecord>) -> Option<JobRecord> {
    let rank = |record: &JobRecord| (record.submission_order, record.queued_at);
    let best = candidates.iter().max_by_key(|record| rank(record));
    let Some(current) = current else {
        return best.cloned();
    };
    let has_copy = candidates.iter().any(|record| record.id == current.id);
    match best {
        Some(best) if has_copy && rank(best) > rank(&current) => Some(best.clone()),
        _ => Some(current),
    }
}

#[cfg(test)]
#[path = "verdict_reuse_tests.rs"]
mod tests;
