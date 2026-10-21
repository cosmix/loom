//! What the daemon does with a finished host job: complete the stage a
//! passing `Complete` run verified, record the checkpoint of a failed one, or
//! hand a `Freeze` job to `freeze::apply_freeze_result`.
//!
//! The orchestrator calls [`apply_finished`] for every record the runner
//! reports and, at startup, for every terminal current record
//! [`unapplied_terminal_records`] lists. A binding or ownership failure is an
//! `Ok` outcome; `Err` is an I/O or state-file failure a later attempt may
//! not hit. Every step is safe to repeat after a crash between the stage
//! transition and the `applied` marker. The marker is written last: the
//! session's waiter (`commands/stage/complete_relay.rs`) reads the marker
//! first and only then the stage and the checkpoint, so the transition and
//! the checkpoints must exist before it does.

use std::fmt;
use std::path::Path;

use anyhow::{Context, Result};

use super::apply_rejection::{
    receipt_recorded, record_acceptance, record_failure, record_rejection,
};
use super::{bounded_detail, freeze, store, JobKind, JobRecord, JobResult, JobState};
use crate::fs::locking::locked_dir_update;
use crate::fs::session_files::load_session_exact;
use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::transitions::{load_stage, update_stage};

/// The job file whose presence marks a job as applied; its content says how.
pub const APPLIED_MARKER: &str = "applied";

/// The session kinds that complete their own stage: a worktree stage session
/// and a knowledge session in the main repository.
const COMPLETING_SESSION_TYPES: &[SessionType] = &[SessionType::Stage, SessionType::Knowledge];

/// How a finished job was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// The stage was completed at `commit`.
    Completed { commit: String },
    /// A checkpoint was written: a failed run, or a passing run a binding
    /// refused.
    Recorded,
    /// The contracts were frozen; `files` is the number the freeze recorded.
    FreezeAccepted { files: usize },
    /// The contract run was refused and its stage parked.
    FreezeRefused(String),
    /// Nothing was applied, for the reason given.
    Ignored(String),
}

/// A binding the passing run no longer satisfies. Raised inside the stage
/// update so the transition is refused and the caller can tell it from an
/// I/O failure.
#[derive(Debug)]
struct BindingFailed(String);

impl fmt::Display for BindingFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BindingFailed {}

fn binding_failed(reason: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(BindingFailed(reason.into()))
}

/// Apply the finished job `record`, then mark it applied. A job already
/// marked, or one still running, is left alone.
pub fn apply_finished(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    if !record.state.is_terminal() {
        return Ok(Applied::Ignored("the job has not finished".to_string()));
    }
    if is_applied(work_dir, record)? {
        return Ok(Applied::Ignored("already applied".to_string()));
    }
    let applied = match record.kind {
        JobKind::Check => Applied::Ignored("a check changes no stage state".to_string()),
        JobKind::Freeze => freeze::apply_freeze_result(work_dir, record)?,
        JobKind::Complete => apply_complete(work_dir, record)?,
    };
    let note = bounded_detail(&describe(&applied));
    store::save_job_file(
        work_dir,
        &record.stage_id,
        &record.id,
        APPLIED_MARKER,
        note.as_bytes(),
    )?;
    Ok(applied)
}

/// Every stage's current record that is terminal and has no `applied`
/// marker: the jobs a crash or a failed application left unapplied. A stage
/// whose state cannot be read is logged and skipped.
pub fn unapplied_terminal_records(work_dir: &Path) -> Result<Vec<JobRecord>> {
    let mut unapplied = Vec::new();
    for stage_id in store::stage_ids(work_dir)? {
        match unapplied_of(work_dir, &stage_id) {
            Ok(Some(record)) => unapplied.push(record),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                stage_id = %stage_id,
                error = %format!("{error:#}"),
                "could not read the stage's host job to find unapplied jobs",
            ),
        }
    }
    Ok(unapplied)
}

fn unapplied_of(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>> {
    let Some(record) = store::load_current(work_dir, stage_id)? else {
        return Ok(None);
    };
    if !record.state.is_terminal() || is_applied(work_dir, &record)? {
        return Ok(None);
    }
    Ok(Some(record))
}

fn is_applied(work_dir: &Path, record: &JobRecord) -> Result<bool> {
    let marker = store::load_job_file(work_dir, &record.stage_id, &record.id, APPLIED_MARKER)?;
    Ok(marker.is_some())
}

/// The marker's content: what the application did, for a waiter to print.
fn describe(applied: &Applied) -> String {
    match applied {
        Applied::Completed { commit } => format!("completed the stage at {commit}"),
        Applied::Recorded => "recorded a checkpoint".to_string(),
        Applied::FreezeAccepted { files } => format!("froze the contracts ({files} files)"),
        Applied::FreezeRefused(message) => format!("refused the contract run: {message}"),
        Applied::Ignored(reason) => reason.clone(),
    }
}

fn apply_complete(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    match record.state {
        JobState::Passed => apply_passed(work_dir, record),
        JobState::Failed => apply_failed(work_dir, record),
        // A cancelled or interrupted run says nothing about the criteria: a
        // checkpoint would replace the `latest` attempt a block reads and
        // spend one of the session's evidence nonces.
        _ => Ok(Applied::Recorded),
    }
}

fn apply_failed(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    let stage = load_stage(&record.stage_id, work_dir)?;
    // Read before judging ownership: a failure to read state is an `Err` a
    // later attempt may not hit, never the verdict that the session lost the
    // stage.
    let session = load_session_exact(work_dir, &record.session_id)?;
    let owned = crate::stage_service::ownership::session_owns_stage_as(
        work_dir,
        &record.stage_id,
        &record.session_id,
        COMPLETING_SESSION_TYPES,
    );
    // The successor of a handed-off, retired or reset session runs its own
    // completion.
    let Some(session) = session.filter(|_| owned.is_ok()) else {
        return Ok(Applied::Ignored(
            "session no longer owns the stage".to_string(),
        ));
    };
    record_failure(work_dir, &stage, &session, record)
}

fn apply_passed(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    let result = store::load_result(work_dir, &record.stage_id, &record.id)?
        .with_context(|| format!("host job {} passed without a result", record.id))?;
    let stage = load_stage(&record.stage_id, work_dir)?;
    if stage.status == StageStatus::Completed {
        return resume_completed(work_dir, &stage, record, &result);
    }
    if let Some(reason) = check_outside_lock(work_dir, record)? {
        return record_rejection(work_dir, &stage, record, &result, &reason);
    }
    match complete_stage(work_dir, record, &result)? {
        Ok(completed) => {
            settle_completed(work_dir, &completed, record, &result)?;
            Ok(Applied::Completed {
                commit: result.head,
            })
        }
        Err(reason) => record_rejection(work_dir, &stage, record, &result, &reason),
    }
}

/// A stage a previous application already completed: finish the steps after
/// the transition, or leave a stage completed from other content alone.
fn resume_completed(
    work_dir: &Path,
    stage: &Stage,
    record: &JobRecord,
    result: &JobResult,
) -> Result<Applied> {
    if stage.completed_commit.as_deref() != Some(result.head.as_str()) {
        return Ok(Applied::Ignored(
            "the stage was completed from other content".to_string(),
        ));
    }
    settle_completed(work_dir, stage, record, result)?;
    Ok(Applied::Ignored("already completed".to_string()))
}

/// The checks that only read, so they run outside the lock: the stage and
/// session bindings, and the completion gates that compare against a
/// recorded fingerprint. `Some(reason)` refuses the run; `Err` is a failure to
/// read state. The gates report a verdict and a git or I/O failure as the same
/// untyped error, so any gate error is a refusal (the gate re-runs in the
/// session's next completion).
fn check_outside_lock(work_dir: &Path, record: &JobRecord) -> Result<Option<String>> {
    load_stage(&record.stage_id, work_dir)?;
    load_session_exact(work_dir, &record.session_id)?;
    if let Err(error) = validate_active_identity(work_dir, &record.stage_id, &record.session_id) {
        return binding_reason(error).map(Some);
    }
    let gates = crate::daemon::check_completion_gates(work_dir, &record.stage_id);
    Ok(gates
        .err()
        .map(|error| format!("completion gates: {error:#}")))
}

/// The reason of a binding failure; any other error is a failure to read or
/// write state and stays an error.
fn binding_reason(error: anyhow::Error) -> Result<String> {
    match error.downcast_ref::<BindingFailed>() {
        Some(failed) => Ok(failed.0.clone()),
        None => Err(error),
    }
}

/// The stage/session binding completion requires: the stage is `Executing`
/// and the session owns it as a stage or knowledge session. Shared with block
/// and dispute through `session_owns_stage_as`; `Executing` is completion's
/// alone.
fn validate_active_identity(work_dir: &Path, stage_id: &str, session_id: &str) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)?;
    if stage.status != StageStatus::Executing {
        return Err(binding_failed(format!(
            "stage '{stage_id}' is not executing"
        )));
    }
    crate::stage_service::ownership::session_owns_stage_as(
        work_dir,
        stage_id,
        session_id,
        COMPLETING_SESSION_TYPES,
    )
    .map_err(|error| binding_failed(format!("{error:#}")))
}

/// Complete the stage under the sessions-directory lock, which canonical
/// session writers also take, from the identity check through the transition.
/// `Ok(Err(reason))` names the binding that failed; `Err` is a failure to
/// read or write state.
fn complete_stage(
    work_dir: &Path,
    record: &JobRecord,
    result: &JobResult,
) -> Result<std::result::Result<Stage, String>> {
    let sessions_dir = work_dir.join("sessions");
    let completed = locked_dir_update(&sessions_dir, || {
        validate_active_identity(work_dir, &record.stage_id, &record.session_id)?;
        update_stage(&record.stage_id, work_dir, |stage| {
            bind_and_complete(stage, work_dir, record, result)
        })
    });
    match completed {
        Ok(stage) => Ok(Ok(stage)),
        Err(error) => binding_reason(error).map(Err),
    }
}

/// Require that the stage is still the one the run verified, then complete
/// it at the run's head.
fn bind_and_complete(
    stage: &mut Stage,
    work_dir: &Path,
    record: &JobRecord,
    result: &JobResult,
) -> Result<()> {
    if stage.status != StageStatus::Executing {
        return Err(binding_failed("stage is no longer executing"));
    }
    if stage.session.as_deref() != Some(record.session_id.as_str()) {
        return Err(binding_failed(
            "stage session changed before completion was applied",
        ));
    }
    if !result.passed || result.head != record.head {
        return Err(binding_failed("the run does not verify the requested head"));
    }
    let snapshot = VerificationSnapshot::capture(stage, work_dir).map_err(|error| {
        binding_failed(format!("completion content is not committed: {error:#}"))
    })?;
    if snapshot.commit != result.head {
        return Err(binding_failed(format!(
            "the run verified {} but the stage's committed content is now {}",
            result.head, snapshot.commit
        )));
    }
    if result.policy_hash.as_deref() != Some(snapshot.policy.as_str()) {
        return Err(binding_failed(
            "the stage's check definition changed since the run",
        ));
    }
    if stage.stage_type == StageType::Knowledge {
        stage.merged = true;
    }
    stage.completed_commit = Some(result.head.clone());
    stage.try_complete(None)
}

/// The steps after the transition, each skipped once done: retiring the
/// session (the monitor then kills it and merges the stage), then the attempt
/// and receipt in the session's checkpoint. A session without a record has
/// neither to update.
fn settle_completed(
    work_dir: &Path,
    stage: &Stage,
    record: &JobRecord,
    result: &JobResult,
) -> Result<()> {
    let Some(session) = load_session_exact(work_dir, &record.session_id)? else {
        tracing::warn!(
            stage_id = %record.stage_id,
            session_id = %record.session_id,
            "the completed stage's session has no record to update",
        );
        return Ok(());
    };
    // Retiring is best effort and cannot fail the step, so it goes first: a
    // checkpoint write that fails then still leaves the session retired, and
    // the retry (`resume_completed`) finishes only the receipt.
    retire_session(work_dir, &session);
    if !receipt_recorded(work_dir, record)? {
        record_acceptance(work_dir, stage, &session, record, result)?;
    }
    Ok(())
}

/// The daemon's half of `commands::stage::session::cleanup_session_resources`:
/// a sandboxed session cannot write its own record or signal file. Best
/// effort, as in the CLI: the stage is already complete. A record that is
/// already terminal was retired by an earlier attempt.
fn retire_session(work_dir: &Path, session: &Session) {
    if session.status.is_terminal() {
        return;
    }
    let completed = SessionStatus::Completed;
    if let Err(error) =
        crate::commands::stage::session::update_session_status(work_dir, &session.id, completed)
    {
        tracing::warn!(session_id = %session.id, error = %format!("{error:#}"), "failed to mark the completed session's record Completed");
    }
    if let Err(error) = crate::orchestrator::signals::remove_signal(&session.id, work_dir) {
        tracing::warn!(session_id = %session.id, error = %format!("{error:#}"), "failed to remove the completed session's signal file");
    }
}
