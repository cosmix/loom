//! Skip and retry commands for stages

use anyhow::{bail, Result};
use std::path::Path;

use crate::models::stage::StageStatus;
use crate::orchestrator::signals::RecoveryReason;
use crate::orchestrator::skip::skip_stage;
use crate::verify::contracts::store::reset_attempts;
use crate::verify::transitions::{load_stage, update_stage};

#[path = "skip_retry_report.rs"]
mod report;

#[path = "skip_retry_recovery.rs"]
mod recovery;

use super::recover::determine_recovery_reason;

/// Skip a stage
pub fn skip(stage_id: String, reason: Option<String>) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;

    skip_stage(&stage_id, reason.clone(), &work_dir)?;

    println!("Stage '{stage_id}' skipped.");
    if let Some(r) = reason {
        println!("Reason: {r}");
    }
    println!("Note: Dependent stages will remain blocked.");

    Ok(())
}

/// Retry a failed, crashed, or hung stage
///
/// Generates a recovery signal with context when the stage was crashed or
/// hung, or when --context is provided. Replaces the old `recover` command.
pub fn retry(stage_id: String, force: bool, context: Option<String>) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    retry_in(&work_dir, stage_id, force, context)
}

fn retry_in(work_dir: &Path, stage_id: String, force: bool, context: Option<String>) -> Result<()> {
    // Every launch holds this from its write-ahead record through its backend's
    // answer. Holding it here, no launch is in flight while the session is
    // judged and none can start until the requeue is written.
    let _admission = crate::fs::stage_admission::lock(work_dir, &stage_id)?;
    let mut stage = load_stage(&stage_id, work_dir)?;
    let original_status = stage.status.clone();
    let original_session = stage.session.clone();

    // A confirmed dead session does not block retry. Unknown ownership does,
    // unless the operator explicitly requests the force override.
    if let Some(ref session_id) = stage.session {
        ensure_retry_session_safe(work_dir, &stage_id, session_id, force)?;
    }

    ensure_retry_allowed(&stage, &stage_id, force)?;
    let recovery_reason = recovery_reason_for(&stage, context.as_deref());

    // Get previous session info before we overwrite it
    let previous_session_id = stage
        .session
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    // Count execution retries separately from stall and contract budgets. Capture
    // the next attempt before --force resets the execution retry budget.
    let recovery_attempt = stage.retry_count.saturating_add(1);

    plan_requeue(&mut stage, force)?;

    let original = (&original_status, original_session.as_deref());
    if let Some(reason) = recovery_reason {
        let recovery = recovery::Recovery {
            reason,
            previous_session_id: &previous_session_id,
            attempt: recovery_attempt,
            context: context.as_deref(),
        };
        recovery::requeue_with_recovery(work_dir, &stage_id, &mut stage, original, force, &recovery)
    } else {
        requeue_without_recovery(work_dir, &stage_id, &mut stage, original, force)
    }
}

/// Refuse a stage whose status cannot be retried, or whose retry budget is
/// spent without `--force`.
fn ensure_retry_allowed(
    stage: &crate::models::stage::Stage,
    stage_id: &str,
    force: bool,
) -> Result<()> {
    // Allow retry for Blocked, CompletedWithFailures, MergeBlocked, NeedsHandoff,
    // and Executing states (the latter two were previously only in `recover`)
    let retryable = matches!(
        stage.status,
        StageStatus::Blocked
            | StageStatus::CompletedWithFailures
            | StageStatus::MergeBlocked
            | StageStatus::NeedsHandoff
            | StageStatus::Executing
    );

    if !retryable {
        bail!(
            "Cannot retry stage in status: {}. \
             Only blocked, completed-with-failures, merge-blocked, \
             needs-handoff, or executing stages can be retried.",
            stage.status
        );
    }

    let max = stage.max_retries.unwrap_or(3);
    if !force && stage.retry_count >= max {
        bail!(
            "Stage '{}' has exceeded retry limit ({}/{}). Use --force to override.",
            stage_id,
            stage.retry_count,
            max
        );
    }
    Ok(())
}

/// Whether the retry needs a recovery signal: explicit --context asks for one,
/// and a close_reason suggesting a crash/hung/context-exhaustion scenario does.
fn recovery_reason_for(
    stage: &crate::models::stage::Stage,
    context: Option<&str>,
) -> Option<RecoveryReason> {
    if context.is_some() {
        return Some(RecoveryReason::Manual);
    }
    // Only auto-generate recovery signal for non-manual reasons
    let auto_reason = determine_recovery_reason(stage);
    (auto_reason != RecoveryReason::Manual).then_some(auto_reason)
}

/// Apply the retry to the in-memory stage the requeue is planned from.
fn plan_requeue(stage: &mut crate::models::stage::Stage, force: bool) -> Result<()> {
    // Reset or increment for retry
    if force {
        stage.retry_count = 0;
        stage.retire_failure(
            "loom stage retry --force",
            crate::models::stage::IssueDisposition::Retried,
        );
    } else {
        stage.retry_count += 1;
    }
    stage.last_failure_at = None;
    // Fresh attempt: clear the previous attempt's start time so the next spawn
    // records a new `started_at`/`attempt_started_at` rather than measuring from
    // the dead session (C-22).
    stage.started_at = None;
    stage.attempt_started_at = None;

    // `Executing -> Queued` is not a valid transition; do the two-step
    // `Executing -> Blocked -> Queued` (mirrors orphan recovery in
    // recovery.rs) so retrying a stage whose session died while still marked
    // Executing succeeds instead of erroring (C-4).
    if stage.status == StageStatus::Executing {
        stage.try_mark_blocked()?;
    }
    stage.try_mark_queued()?;
    Ok(())
}

/// Simple retry without recovery signal. Clear the stale session pointer so the
/// executor spawns a fresh session rather than mistaking the dead session's ID
/// for a live/recovery one (C-22).
fn requeue_without_recovery(
    work_dir: &Path,
    stage_id: &str,
    stage: &mut crate::models::stage::Stage,
    original: (&StageStatus, Option<&str>),
    force: bool,
) -> Result<()> {
    stage.session = None;
    stage.updated_at = chrono::Utc::now();
    persist_retry_delta(stage_id, work_dir, stage, original, force)?;

    println!("Stage '{stage_id}' queued for retry.");
    if force {
        println!("Retry count reset (--force used).");
    }
    Ok(())
}

fn ensure_retry_session_safe(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    force: bool,
) -> Result<()> {
    match crate::orchestrator::session_registry::assigned_session_is_live_admitted(
        work_dir, stage_id, session_id,
    ) {
        Ok(false) => Ok(()),
        Ok(true) => {
            eprintln!("WARNING: Stage '{stage_id}' has an active (live) session ({session_id})");
            eprintln!("  If the session is still running, retry will create a parallel session.");
            eprintln!("  Fix issues in the current session and run 'loom stage complete {stage_id}'.");
            if !force {
                bail!("Stage has a live session. Use --force to override.");
            }
            eprintln!("  --force used, proceeding with retry despite live session.");
            Ok(())
        }
        Err(error) if !force => bail!(
            "Cannot establish whether session '{session_id}' is live: {error:#}. Use --force to override."
        ),
        Err(error) => {
            eprintln!("WARNING: Session '{session_id}' liveness is unknown: {error:#}");
            Ok(())
        }
    }
}

fn persist_retry_delta(
    stage_id: &str,
    work_dir: &Path,
    planned: &crate::models::stage::Stage,
    original: (&StageStatus, Option<&str>),
    force: bool,
) -> Result<()> {
    update_stage(stage_id, work_dir, |current| {
        validate_retry_transition(stage_id, current, original, force)?;
        apply_retry_delta(current, planned, force)?;
        // Reset last, once every check above has succeeded and the requeue
        // itself is settled: a refused transition (status changed, retry
        // limit exceeded, or an illegal status transition) must never reset
        // the budget, and a failed reset must abort the transition rather
        // than leave it requeued with a still-spent budget.
        reset_contract_budget(current, work_dir)?;
        Ok(())
    })?;
    // The requeued stage starts a fresh attempt; a run for the old one has
    // nobody left to report to. The host-job store's stage lock makes this
    // write safe against the daemon's runner.
    if let Err(error) = crate::host_verify::submit::cancel(work_dir, stage_id, "stage retried") {
        eprintln!("Warning: could not cancel the host job of stage '{stage_id}': {error:#}");
    }
    Ok(())
}

/// Refuse a retry whose on-disk state no longer matches what the caller
/// planned against: the stage changed status or session while retry was being
/// prepared, or it has exceeded its retry limit without `--force`.
fn validate_retry_transition(
    stage_id: &str,
    current: &crate::models::stage::Stage,
    (original_status, original_session): (&StageStatus, Option<&str>),
    force: bool,
) -> Result<()> {
    if &current.status != original_status {
        bail!(
            "Stage '{}' changed from {} to {} while retry was being prepared; retry again",
            stage_id,
            original_status,
            current.status
        );
    }
    if current.session.as_deref() != original_session {
        bail!("Stage '{stage_id}' changed session while retry was being prepared; retry again");
    }

    let max = current.max_retries.unwrap_or(3);
    if !force && current.retry_count >= max {
        bail!(
            "Stage '{}' has exceeded retry limit ({}/{}). Use --force to override.",
            stage_id,
            current.retry_count,
            max
        );
    }
    Ok(())
}

/// Apply the retry delta to the freshly-validated on-disk stage: reset or
/// increment the retry counter, clear the previous attempt's timing, and
/// requeue it.
pub(super) fn apply_retry_delta(
    current: &mut crate::models::stage::Stage,
    planned: &crate::models::stage::Stage,
    force: bool,
) -> Result<()> {
    if force {
        current.retry_count = 0;
        current.retire_failure(
            "loom stage retry --force",
            crate::models::stage::IssueDisposition::Retried,
        );
    } else {
        current.retry_count += 1;
    }
    current.last_failure_at = None;
    // A retry is a fresh attempt: the stall recovery budget starts over.
    current.stall_recoveries = 0;
    current.started_at = None;
    current.attempt_started_at = None;
    if current.status == StageStatus::Executing {
        current.try_mark_blocked()?;
    }
    current.try_mark_queued()?;
    current.session = planned.session.clone();
    current.close_reason = planned.close_reason.clone();
    current.updated_at = chrono::Utc::now();
    Ok(())
}

/// A human retry or review approval grants a fresh contract respawn budget:
/// left spent, the first contract writer to end without freezing would send
/// the stage straight back to human review. Called from inside the locked
/// `update_stage` closure that performs the requeue, after that closure's own
/// status re-validation succeeds, so a refused transition never resets the
/// budget and a failed reset aborts the transition instead of leaving it
/// requeued with a still-spent budget. A frozen stage keeps its count.
/// Shared with `human_review::handle_approve`.
pub(super) fn reset_contract_budget(
    stage: &crate::models::stage::Stage,
    work_dir: &Path,
) -> Result<()> {
    if stage.contracts.is_empty() {
        return Ok(());
    }
    reset_attempts(work_dir, &stage.id)
}

#[cfg(test)]
#[path = "skip_retry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "skip_retry_host_job_tests.rs"]
mod host_job_tests;
