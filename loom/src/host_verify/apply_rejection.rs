//! The completion attempts the application of a `Complete` job records in its
//! session's completion checkpoint (`handoff::completion::record`), each one
//! naming the job id as its evidence nonce and the job's head as its commit:
//!
//! - a passing run the daemon completed from: a `VerifiedPendingAck` attempt,
//!   then the accepted receipt;
//! - a passing run a binding refused: the same attempt as a `DaemonRejected`
//!   boundary failure (failure code `daemon_rejected`), which repeated-blocker
//!   escalation counts;
//! - a failed run: a `ToolFailed` attempt with one criterion per outcome, so
//!   a block is still refused while an `acceptance-<i>` fails.
//!
//! The attempts carry the stage environment policy and the host's `TARGET_OS`.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};

use super::apply::Applied;
use super::{new_job_id, store, CriterionOutcome, JobRecord, JobResult};
use crate::fs::session_files::load_session_exact;
use crate::handoff::completion::policy::effective_policy_hash;
use crate::handoff::{
    record_accepted_handoff, record_attempt_handoff, AcceptedReceipt, CompletionAttemptEvidence,
    CompletionPhase, CriterionResult, EnvironmentFact, VerificationCheckpoint,
    COMPLETION_EVIDENCE_VERSION, MAX_CRITERIA, MAX_TEXT_LEN, STAGE_ENVIRONMENT_POLICY,
};
use crate::models::session::Session;
use crate::models::stage::Stage;

/// The boundary failure code of a passing run the daemon refused to apply.
const DAEMON_REJECTED: &str = "daemon_rejected";

/// Record the passing run `record` the daemon completed `stage` from: its
/// attempt, then the receipt that names it (`record_accepted_handoff` refuses
/// a receipt whose nonce no observation carries).
pub(super) fn record_acceptance(
    work_dir: &Path,
    stage: &Stage,
    session: &Session,
    record: &JobRecord,
    result: &JobResult,
) -> Result<()> {
    let policy = result
        .policy_hash
        .as_deref()
        .context("a passing completion run carries its policy hash")?;
    let verified = attempt(
        stage,
        record,
        policy,
        CompletionPhase::VerifiedPendingAck,
        result,
    );
    record_attempt_handoff(session, stage, &verified, work_dir)?;
    let receipt = AcceptedReceipt {
        evidence_nonce: record.id.clone(),
        completion_nonce: new_job_id(),
        commit: result.head.clone(),
        attestation: None,
    };
    record_accepted_handoff(session, stage, receipt, work_dir)?;
    Ok(())
}

/// Whether the session's checkpoint already holds the receipt of `record`.
pub(super) fn receipt_recorded(work_dir: &Path, record: &JobRecord) -> Result<bool> {
    let checkpoint =
        crate::handoff::load_session_checkpoint(&record.stage_id, &record.session_id, work_dir)?;
    Ok(checkpoint
        .and_then(|checkpoint| checkpoint.accepted)
        .is_some_and(|receipt| receipt.evidence_nonce == record.id))
}

/// Record that the passing run `record` was not applied because `reason`, a
/// binding, failed. A session without a record has no checkpoint to tell.
pub(super) fn record_rejection(
    work_dir: &Path,
    stage: &Stage,
    record: &JobRecord,
    result: &JobResult,
    reason: &str,
) -> Result<Applied> {
    tracing::warn!(
        stage_id = %record.stage_id,
        job_id = %record.id,
        reason = %reason,
        "The daemon refused to complete a stage from a passing host job"
    );
    let Some(session) = load_session_exact(work_dir, &record.session_id)? else {
        return Ok(Applied::Ignored(format!(
            "not completed ({reason}); session '{}' has no record",
            record.session_id
        )));
    };
    let policy = policy_of(stage, result, work_dir)?;
    let verified = attempt(
        stage,
        record,
        &policy,
        CompletionPhase::VerifiedPendingAck,
        result,
    );
    let rejected = with_boundary_failure(
        &verified,
        CompletionPhase::DaemonRejected,
        DAEMON_REJECTED,
        Some(reason),
    );
    record_attempt_handoff(&session, stage, &rejected, work_dir)?;
    Ok(Applied::Recorded)
}

/// Write the completion checkpoint of the failed run `record` for `session`:
/// its outcomes (the result's, else the progress the run recorded) and the
/// first failure as the diagnostic.
pub(super) fn record_failure(
    work_dir: &Path,
    stage: &Stage,
    session: &Session,
    record: &JobRecord,
) -> Result<Applied> {
    let result = store::load_result(work_dir, &record.stage_id, &record.id)?;
    let result = match result {
        Some(result) => result,
        None => JobResult {
            job_id: record.id.clone(),
            head: record.head.clone(),
            passed: false,
            criteria: store::read_progress(work_dir, &record.stage_id, &record.id)?,
            error: None,
            policy_hash: None,
        },
    };
    let policy = policy_of(stage, &result, work_dir)?;
    let mut failed = attempt(stage, record, &policy, CompletionPhase::ToolFailed, &result);
    let diagnostic = first_failure(&result.criteria)
        .or_else(|| result.error.clone())
        .or_else(|| record.detail.clone());
    failed.diagnostic_first_line = bounded_diagnostic(diagnostic.as_deref());
    record_attempt_handoff(session, stage, &failed, work_dir)?;
    Ok(Applied::Recorded)
}

/// `evidence` as a failure at the daemon boundary: `phase`, `failure_code`
/// and the first line of `diagnostic`, observed now.
fn with_boundary_failure(
    evidence: &CompletionAttemptEvidence,
    phase: CompletionPhase,
    failure_code: &str,
    diagnostic: Option<&str>,
) -> CompletionAttemptEvidence {
    let mut failed = evidence.clone();
    failed.phase = phase;
    failed.external_failure_code = Some(failure_code.to_string());
    failed.diagnostic_first_line = bounded_diagnostic(diagnostic);
    failed.observed_at = observed_at();
    failed
}

/// The attempt of job `record` in `phase`, one criterion per outcome of
/// `result`, bound to `policy`.
fn attempt(
    stage: &Stage,
    record: &JobRecord,
    policy: &str,
    phase: CompletionPhase,
    result: &JobResult,
) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: record.session_id.clone(),
        commit: record.head.clone(),
        check_definition_hash: policy.to_string(),
        exact_command: format!("loom stage complete {}", stage.id),
        evidence_nonce: record.id.clone(),
        verification: VerificationCheckpoint {
            criteria: criteria(&result.criteria),
            environment_policy: STAGE_ENVIRONMENT_POLICY.to_string(),
            environment: vec![EnvironmentFact {
                name: "TARGET_OS".to_string(),
                value: std::env::consts::OS.to_string(),
            }],
        },
        phase,
        external_failure_code: None,
        diagnostic_first_line: None,
        observed_at: observed_at(),
        attestation: None,
    }
}

/// The policy the run was bound to, else the stage's current one.
fn policy_of(stage: &Stage, result: &JobResult, work_dir: &Path) -> Result<String> {
    match &result.policy_hash {
        Some(policy) => Ok(policy.clone()),
        None => effective_policy_hash(stage, work_dir),
    }
}

/// One criterion per outcome label, in run order. Past `MAX_CRITERIA` the
/// failures are kept first: a block is refused while one of them fails.
fn criteria(outcomes: &[CriterionOutcome]) -> Vec<CriterionResult> {
    let mut seen = HashSet::new();
    let mut unique: Vec<&CriterionOutcome> = outcomes
        .iter()
        .filter(|outcome| seen.insert(outcome.label.as_str()))
        .collect();
    if unique.len() > MAX_CRITERIA {
        unique.sort_by_key(|outcome| outcome.passed);
        unique.truncate(MAX_CRITERIA);
    }
    unique
        .into_iter()
        .map(|outcome| CriterionResult {
            id: outcome.label.clone(),
            passed: outcome.passed,
        })
        .collect()
}

/// `<label>: <first line of its summary>` for the first failed outcome.
fn first_failure(outcomes: &[CriterionOutcome]) -> Option<String> {
    let outcome = outcomes.iter().find(|outcome| !outcome.passed)?;
    let line = outcome
        .summary
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    Some(format!("{}: {line}", outcome.label))
}

fn bounded_diagnostic(diagnostic: Option<&str>) -> Option<String> {
    diagnostic.map(|text| {
        text.lines()
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control())
            .take(MAX_TEXT_LEN)
            .collect()
    })
}

fn observed_at() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
