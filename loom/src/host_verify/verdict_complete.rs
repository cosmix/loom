//! The completion the daemon runs for a stage a verdict left held
//! (`JobKind::VerdictComplete`): `NeedsAdjudication`, no session, every
//! dispute applied. Submission refuses a stage the completion gates refuse,
//! completes it from the stage's latest `loom stage complete` run when that
//! run passed on its committed tree (`verdict_reuse`), and otherwise queues
//! one job bound to the stage when that run still counts for the tree;
//! application completes the stage from a passing run, or hands it back to a
//! fresh session (`adjudication::hand_back_held_stage`).
//!
//! No session owns the job, so nothing here loads, retires or checkpoints
//! one: the receipt of a completion is `stage.completed_commit` and the job's
//! `applied` marker.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use super::apply::{binding_failed, binding_reason, verify_bound_run, Applied, APPLIED_MARKER};
use super::apply_rejection::first_summary_line;
use super::submit::{submit, Submission, SubmitRequest};
use super::verdict_reuse::{self, latest_of};
use super::{digest, gates, store, JobKind, JobRecord, JobResult, JobState};
use crate::fs::dispute_lock::lock_stage_dispute_dir_for_daemon;
use crate::git::worktree::{stage_worktree_path, WorktreeGit};
use crate::handoff::completion::content::committed_content;
use crate::handoff::completion::policy::effective_policy_hash;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::adjudication::{
    hand_back_held_stage, is_held_for_completion, is_verdict_held,
};
use crate::verify::completion::IMPACT_TESTS_LABEL;
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::transitions::{load_stage, update_stage};

/// The record `session_id` of a job bound to its stage instead of a session.
pub(crate) const STAGE_BOUND_SESSION: &str = "daemon";
/// The relay request id a verdict completion job records.
pub(crate) const VERDICT_REQUEST_ID: &str = "verdict-completion";

const NOT_HELD: &str = "the stage is no longer held for its verdict completion";
const ON_HOLD: &str = "it is on hold, so loom does not run its completion";

/// What submitting a held stage's completion did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VerdictStart {
    /// The stage is not verdict-held (a reset, a new dispute, a session):
    /// nothing was queued, and no hand-back is owed.
    NotHeld,
    /// The stage's verdict completion job, not yet finished or applied.
    Waiting(String),
    /// The job queued for the stage.
    Submitted(String),
    /// The stage was completed at `commit` from job `job`, its latest
    /// `loom stage complete` run, which passed there; nothing was queued.
    Reused { job: String, commit: String },
    /// The completion gates refuse the stage, so no run could complete it;
    /// nothing was queued, and the refusal is the fresh session's failure.
    GatesRefused(String),
    /// Why the stage needs a fresh session instead; nothing was queued.
    Ineligible(String),
}

/// Queue the completion run of the verdict-held `stage` at its committed
/// HEAD, unless it is not verdict-held, a job already decides it, or the
/// stage cannot be completed without a session (`Ineligible`), as when the
/// operator holds it. A deciding job is waited for even under the operator's
/// hold: its child still runs in the worktree, so the stage goes back only
/// once the job's application is recorded.
pub(crate) fn submit_verdict_completion(
    work_dir: &Path,
    stage: &Stage,
    now: DateTime<Utc>,
) -> Result<VerdictStart> {
    if !is_verdict_held(work_dir, stage) {
        return Ok(VerdictStart::NotHeld);
    }
    if let Some(job) = deciding_job(work_dir, &stage.id)? {
        return Ok(VerdictStart::Waiting(job));
    }
    if stage.held {
        return Ok(VerdictStart::Ineligible(ON_HOLD.to_string()));
    }
    if !crate::verify::review::gate::covers(stage) {
        return Ok(VerdictStart::Ineligible(format!(
            "stage '{}' is not a plan v2 standard or integration-verify stage",
            stage.id
        )));
    }
    let Some(latest) = latest_of(work_dir, &stage.id, &[JobKind::Complete])? else {
        return Ok(VerdictStart::Ineligible(
            "no `loom stage complete` request of the stage is on record".to_string(),
        ));
    };
    match counted_result(work_dir, stage, &latest)? {
        Ok(result) => start_at_requested_head(work_dir, stage, &latest, &result, now),
        Err(reason) => Ok(VerdictStart::Ineligible(reason.to_string())),
    }
}

/// The id of the stage's current verdict completion job when it still
/// decides the stage: not finished, or finished and not yet applied.
fn deciding_job(work_dir: &Path, stage_id: &str) -> Result<Option<String>> {
    let Some(record) = store::load_current(work_dir, stage_id)? else {
        return Ok(None);
    };
    if record.kind != JobKind::VerdictComplete {
        return Ok(None);
    }
    let applied = store::load_job_file(work_dir, stage_id, &record.id, APPLIED_MARKER)?.is_some();
    let settled = record.state.is_terminal() && applied;
    Ok((!settled).then_some(record.id))
}

/// The result of the latest completion run `record` when it can stand for a
/// completion without a session, else why it cannot: it left a result bound
/// to the stage's current check definition, and it passed or failed only its
/// impact tests. A sibling verdict that amended criteria, re-froze contracts
/// or changed the plan moves the policy, and the stage then needs a session
/// that reads the new one.
fn counted_result(
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
fn run_counts(state: JobState, result: &JobResult) -> bool {
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

/// When the stage's worktree is committed and clean at the head of `latest`,
/// its latest completion request: refuse the stage the gates refuse, complete
/// it from `latest` when that run decides it, else queue the job. The gates
/// read state no run changes, so a refusal now is the one the application of
/// a run would give.
fn start_at_requested_head(
    work_dir: &Path,
    stage: &Stage,
    latest: &JobRecord,
    result: &JobResult,
    now: DateTime<Utc>,
) -> Result<VerdictStart> {
    let repo_root = crate::fs::work_dir::WorkDir::new(work_dir)?
        .main_project_root()
        .context("the repository of the state root is unavailable")?;
    let head = match committed_content(stage, &repo_root) {
        Ok(head) => head,
        Err(error) => {
            return Ok(VerdictStart::Ineligible(format!(
                "the stage's worktree is not committed and clean: {error:#}"
            )));
        }
    };
    if head != latest.head {
        return Ok(VerdictStart::Ineligible(format!(
            "HEAD {head} is not {}, the commit its latest completion request named",
            latest.head
        )));
    }
    if let Some(refusal) = gates::refusal(work_dir, &stage.id) {
        return Ok(VerdictStart::GatesRefused(refusal));
    }
    if verdict_reuse::decides(work_dir, latest, result)? {
        return verdict_reuse::complete_from_run(work_dir, latest, result);
    }
    submit_at(work_dir, stage, &repo_root, &head, now)
}

/// Queue the stage-bound job at `head`.
fn submit_at(
    work_dir: &Path,
    stage: &Stage,
    repo_root: &Path,
    head: &str,
    now: DateTime<Utc>,
) -> Result<VerdictStart> {
    // The digest the child compares against, computed through the same
    // pinned git (`child::JobTree`).
    let worktree = stage_worktree_path(stage, repo_root)?;
    let tree_digest = digest::tree_digest_in(&WorktreeGit::pinned(repo_root, &worktree)?)?;
    let request = SubmitRequest {
        stage_id: &stage.id,
        session_id: STAGE_BOUND_SESSION,
        request_id: VERDICT_REQUEST_ID,
        kind: JobKind::VerdictComplete,
        head,
        tree_digest: &tree_digest,
        no_cache: false,
        criteria: None,
    };
    Ok(match submit(work_dir, &request, now)? {
        Submission::Queued(job) | Submission::Joined(job) => VerdictStart::Submitted(job.id),
        Submission::Waiting { pending, .. } => VerdictStart::Submitted(pending.id),
        Submission::Superseded { queued, .. } => VerdictStart::Submitted(queued.id),
        // A refusal is deterministic: a fresh session beats retrying it every tick.
        Submission::Refused(reason) => VerdictStart::Ineligible(reason),
    })
}

/// The head of the stage's latest completion request.
#[cfg(test)]
pub(crate) fn latest_completion_head(work_dir: &Path, stage_id: &str) -> Result<Option<String>> {
    Ok(latest_of(work_dir, stage_id, &[JobKind::Complete])?.map(|record| record.head))
}

/// Apply the finished verdict completion `record`: complete the held stage
/// from a passing run, or hand it back to a fresh session.
pub(super) fn apply_verdict_completion(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    let stage = load_stage(&record.stage_id, work_dir)?;
    if stage.status == StageStatus::Completed {
        // An application that crashed before its marker resumes here.
        let reason = if stage.completed_commit.as_deref() == Some(record.head.as_str()) {
            "already completed"
        } else {
            "the stage was completed from other content"
        };
        return Ok(Applied::Ignored(reason.to_string()));
    }
    if let Some(reason) = not_deciding(work_dir, &stage, record)? {
        return Ok(Applied::Ignored(reason));
    }
    match record.state {
        JobState::Passed => apply_passed(work_dir, record),
        JobState::Failed => hand_back(work_dir, record, failure_lines(work_dir, record)?),
        JobState::Cancelled | JobState::Interrupted => hand_back(work_dir, record, Vec::new()),
        JobState::Queued | JobState::Running => {
            Ok(Applied::Ignored("the job has not finished".to_string()))
        }
    }
}

/// Why `record` no longer decides `stage`, if it does not: the stage left its
/// verdict hold (a dispute filed meanwhile, a reset, a session, the
/// operator's hold, which the next sweep hands back), or another unfinished
/// job is the stage's current one.
fn not_deciding(work_dir: &Path, stage: &Stage, record: &JobRecord) -> Result<Option<String>> {
    if !is_held_for_completion(work_dir, stage) {
        return Ok(Some(NOT_HELD.to_string()));
    }
    let current = store::load_current(work_dir, &record.stage_id)?;
    Ok(current
        .filter(|current| current.id != record.id && !current.state.is_terminal())
        .map(|current| format!("host job {} decides the stage", current.id)))
}

fn apply_passed(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    let result = store::load_result(work_dir, &record.stage_id, &record.id)?
        .with_context(|| format!("host job {} passed without a result", record.id))?;
    if let Some(refusal) = gates::refusal(work_dir, &record.stage_id) {
        return hand_back(work_dir, record, vec![refusal]);
    }
    match complete_held(work_dir, record, &result)? {
        Ok(()) => Ok(Applied::Completed {
            commit: result.head,
        }),
        Err(reason) => hand_back(work_dir, record, vec![reason]),
    }
}

/// Complete the held stage from the passing `result` of `record`, under the
/// stage's dispute claim and its stage file. `Ok(Err(reason))` names the
/// binding that failed; `Err` is a failure to read or write state. The claim
/// drops before this returns, so the caller may hand the stage back, which
/// takes it again: two `flock` descriptors in one process conflict.
pub(super) fn complete_held(
    work_dir: &Path,
    record: &JobRecord,
    result: &JobResult,
) -> Result<std::result::Result<(), String>> {
    let completed = {
        let disputes = work_dir.join("disputes").join(&record.stage_id);
        let _claim = lock_stage_dispute_dir_for_daemon(&disputes)?;
        update_stage(&record.stage_id, work_dir, |stage| {
            bind_and_complete_held(stage, work_dir, record, result)
        })
    };
    match completed {
        Ok(_) => {
            tracing::info!(stage = %record.stage_id, job = %record.id, commit = %result.head,
                "completed the verdict-held stage from the daemon's run");
            Ok(Ok(()))
        }
        Err(error) => binding_reason(error).map(Err),
    }
}

/// Under the dispute and stage locks: the stage is still held for this
/// completion and is still the content the run verified.
fn bind_and_complete_held(
    stage: &mut Stage,
    work_dir: &Path,
    record: &JobRecord,
    result: &JobResult,
) -> Result<()> {
    if !is_held_for_completion(work_dir, stage) {
        return Err(binding_failed(
            "the stage left its verdict hold before completion was applied",
        ));
    }
    verify_bound_run(stage, work_dir, record, result)?;
    stage.completed_commit = Some(result.head.clone());
    stage.complete_from_verdict()
}

/// Re-queue the held stage, appending `failures` to its feedback.
fn hand_back(work_dir: &Path, record: &JobRecord, failures: Vec<String>) -> Result<Applied> {
    if !hand_back_held_stage(work_dir, &record.stage_id, &failures)? {
        return Ok(Applied::Ignored(NOT_HELD.to_string()));
    }
    let reason = failures
        .into_iter()
        .next()
        .unwrap_or_else(|| format!("the run was {:?}", record.state).to_lowercase());
    Ok(Applied::HandedBack(reason))
}

/// One line per failed check of the run (the result's, else the progress it
/// recorded), then its pipeline error, else the record's detail.
fn failure_lines(work_dir: &Path, record: &JobRecord) -> Result<Vec<String>> {
    let (outcomes, error) = match store::load_result(work_dir, &record.stage_id, &record.id)? {
        Some(result) => (result.criteria, result.error),
        None => {
            let progress = store::read_progress(work_dir, &record.stage_id, &record.id)?;
            (progress, None)
        }
    };
    let mut lines: Vec<String> = outcomes
        .iter()
        .filter(|outcome| !outcome.passed)
        .map(|outcome| {
            let line = first_summary_line(&outcome.summary);
            format!("{}: {line}", outcome.label)
        })
        .collect();
    lines.extend(error.or_else(|| record.detail.clone()));
    if lines.is_empty() {
        lines.push("the run failed without naming a check".to_string());
    }
    Ok(lines)
}

/// Hand the held stage back once the application of its job gave up, when
/// the job still decides it. An `Err` leaves the job unmarked, so the next
/// tick retries the hand-back.
pub(super) fn give_up(work_dir: &Path, record: &JobRecord, reason: &str) -> Result<()> {
    let stage = load_stage(&record.stage_id, work_dir)?;
    if not_deciding(work_dir, &stage, record)?.is_some() {
        return Ok(());
    }
    let failure = format!(
        "loom could not apply its completion run: {}",
        run_output_line(reason)
    );
    hand_back_held_stage(work_dir, &record.stage_id, &[failure])?;
    Ok(())
}

#[cfg(test)]
#[path = "verdict_complete_tests.rs"]
mod tests;
