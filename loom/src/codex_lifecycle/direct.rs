//! Direct-mode Codex forwards: `codex exec` run by the forwarding wrapper
//! without a companion job, so the forward receipt is the only evidence.
//!
//! The receipt hook writes a direct forward's `running` and terminal rows
//! together at PostToolUse, after the run ends; there is no receipt while the
//! forward runs. Companion forwards get receipts at the same point, so a
//! missing receipt says nothing about the backend: reconcile looks for a
//! companion job first and only then applies [`RECEIPT_WINDOW_SECS`].

use std::path::Path;

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};

use crate::models::forward_receipt::{
    load_receipts, receipts_path, ForwardBackend, ForwardIdentity, ForwardReceipt, ForwardState,
    ReceiptLoad,
};
use crate::subagent_lifecycle::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleState, WorkerIdentity,
};

use super::authorization::CodexAuthorization;
use super::jobs::JobObservation;

/// How long an authorization may have neither a forward receipt nor a
/// companion job before reconcile reports it unknown: the wrapper's 540 s unit
/// deadline plus slack for the PostToolUse receipt write.
pub(super) const RECEIPT_WINDOW_SECS: i64 = 600;

/// Entry detail for an authorization still inside the receipt window.
pub(super) const AWAITING_RECEIPT: &str = "awaiting forward receipt or companion job";

/// How one authorization's forward receipt routes reconcile.
pub(super) enum DirectOutcome {
    /// A direct receipt: record this execution and observation.
    Direct(CodexExecution, JobObservation),
    /// A companion receipt: the companion job is the evidence.
    Companion,
    /// No receipt yet: try the companion job, then the receipt window.
    NoReceipt,
    /// The receipt evidence cannot be trusted; nothing is recorded.
    Unknown(String),
}

/// One stage's forward receipts, loaded once per reconcile pass.
pub(super) struct StageReceipts {
    load: Result<ReceiptLoad, String>,
    /// The reconcile clock, which decides only the receipt window.
    pub now: DateTime<Utc>,
}

impl StageReceipts {
    pub fn load(work_dir: &Path, stage_id: &str, now: DateTime<Utc>) -> Self {
        let load = receipts_path(work_dir, stage_id)
            .and_then(|path| load_receipts(&path))
            .map_err(|error| format!("reading forward receipts: {error:#}"));
        Self { load, now }
    }

    pub fn receipts(&self) -> Result<&ReceiptLoad, &str> {
        self.load.as_ref().map_err(String::as_str)
    }
}

/// Classify the receipt bound to `row` by its exact receipt id.
///
/// Only the folded receipt state decides the outcome: a conflicting history
/// folds to `Unknown` without clearing an earlier exit code, so a set
/// `exit_code` never makes a receipt terminal.
pub(super) fn receipt_outcome(row: &CodexAuthorization, receipts: &ReceiptLoad) -> DirectOutcome {
    if receipts.truncated || receipts.malformed > 0 {
        return DirectOutcome::Unknown("forward receipt journal is damaged".into());
    }
    let identity = match ForwardIdentity::new(
        row.parent_session_id.clone(),
        row.forwarder_agent_id.clone(),
        row.tool_use_id.clone(),
        row.stage_id.clone(),
        row.loom_session_id.clone(),
    ) {
        Ok(identity) => identity,
        Err(error) => return DirectOutcome::Unknown(error.to_string()),
    };
    let Some(receipt) = receipts.get(&identity.receipt_id()) else {
        return DirectOutcome::NoReceipt;
    };
    if !receipt_matches_authorization(receipt, row) {
        return DirectOutcome::Unknown("forward receipt disagrees with its authorization".into());
    }
    match receipt.backend {
        ForwardBackend::Companion => DirectOutcome::Companion,
        ForwardBackend::Direct => direct_observation(receipt, row),
    }
}

fn direct_observation(receipt: &ForwardReceipt, row: &CodexAuthorization) -> DirectOutcome {
    let (state, outcome, detail) = match receipt.state {
        ForwardState::Running => (LifecycleState::Running, CodexEvidenceOutcome::Running, None),
        ForwardState::Succeeded => (
            LifecycleState::Completed,
            CodexEvidenceOutcome::Succeeded,
            None,
        ),
        ForwardState::Failed => (
            LifecycleState::Failed,
            CodexEvidenceOutcome::Failed,
            Some(receipt.exit_code.map_or_else(
                || "direct Codex forward failed".to_owned(),
                |code| format!("direct Codex forward failed with exit code {code}"),
            )),
        ),
        ForwardState::Canceled | ForwardState::TimedOut => (
            LifecycleState::Cancelled,
            CodexEvidenceOutcome::Cancelled,
            Some(format!("direct Codex forward {}", receipt.state.label())),
        ),
        ForwardState::Queued | ForwardState::Unknown => {
            return DirectOutcome::Unknown(format!(
                "direct forward receipt is {}",
                receipt.state.label()
            ))
        }
    };
    let terminal_at = (state != LifecycleState::Running).then_some(receipt.observed_at);
    let execution = CodexExecution::Direct {
        thread_id: receipt.backend_id.clone(),
        tool_use_id: row.tool_use_id.clone(),
    };
    let observation = JobObservation {
        state,
        outcome,
        thread_id: Some(receipt.backend_id.clone()),
        turn_id: None,
        terminal_at,
        detail,
    };
    DirectOutcome::Direct(execution, observation)
}

/// The authorization evidence, carrying the ids the lifecycle validator
/// requires of each execution mode: a companion job id, or a direct thread
/// and tool use and no job id.
pub(super) fn base_evidence(
    authorization: &CodexAuthorization,
    identity: &WorkerIdentity,
) -> CodexEvidence {
    let (job_id, thread_id, tool_use_id) = match identity {
        WorkerIdentity::Codex {
            execution:
                CodexExecution::Direct {
                    thread_id,
                    tool_use_id,
                },
            ..
        } => (None, Some(thread_id.clone()), Some(tool_use_id.clone())),
        WorkerIdentity::Codex {
            execution: CodexExecution::Companion { job_id },
            ..
        } => (Some(job_id.clone()), None, None),
        _ => (None, None, None),
    };
    CodexEvidence {
        evidence_kind: CodexEvidenceKind::Authorization,
        requested_model: authorization.model.clone(),
        requested_effort: authorization.effort.clone(),
        invocation_id: authorization.invocation_id.clone(),
        job_id,
        thread_id,
        turn_id: None,
        tool_use_id,
        terminal_at: None,
        outcome: CodexEvidenceOutcome::Running,
        detail: None,
    }
}

/// The lifecycle producer the validator pairs with an execution mode.
pub(super) fn producer_of(identity: &WorkerIdentity) -> LifecycleProducer {
    match identity {
        WorkerIdentity::Codex {
            execution: CodexExecution::Direct { .. },
            ..
        } => LifecycleProducer::CodexDirect,
        _ => LifecycleProducer::CodexCompanion,
    }
}

/// Whether a receipt belongs to an authorization: the exact forward identity
/// and, where the receipt names them, the requested model and effort.
pub(crate) fn receipt_matches_authorization(
    receipt: &ForwardReceipt,
    authorization: &CodexAuthorization,
) -> bool {
    receipt.identity.stage_id == authorization.stage_id
        && receipt.identity.loom_session_id == authorization.loom_session_id
        && receipt.identity.parent_session_id == authorization.parent_session_id
        && receipt.identity.agent_id == authorization.forwarder_agent_id
        && receipt.identity.tool_use_id == authorization.tool_use_id
        && receipt
            .model
            .as_deref()
            .is_none_or(|value| value == authorization.model)
        && receipt
            .effort
            .as_deref()
            .is_none_or(|value| value == authorization.effort)
}

/// Decide an authorization with no receipt and no companion job: `Ok` while
/// it is inside the receipt window, else unknown.
pub(super) fn receipt_window(
    row: &CodexAuthorization,
    job_error: &anyhow::Error,
    now: DateTime<Utc>,
) -> Result<()> {
    if now < row.authorized_at + Duration::seconds(RECEIPT_WINDOW_SECS) {
        tracing::debug!(
            stage = %row.stage_id,
            unit = %row.unit_id,
            error = %job_error,
            "Codex forward has no receipt or companion job yet"
        );
        return Ok(());
    }
    bail!("no forward receipt {RECEIPT_WINDOW_SECS} s after authorization; companion job: {job_error}")
}

#[cfg(test)]
#[path = "direct_tests.rs"]
mod tests;
