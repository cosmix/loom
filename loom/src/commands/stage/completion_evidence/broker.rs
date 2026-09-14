use std::path::Path;

use anyhow::Result;

use super::{
    diagnostic_evidence, fresh_nonce, parse_evidence_record, pinned_command, record_evidence,
    validate_against, with_boundary_failure, RecordRoute, OUTCOME_PREFIX,
};
use crate::daemon::Response;
use crate::handoff::{load_trusted_session_checkpoint, CompletionAttemptEvidence, CompletionPhase};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::load_stage;

#[cfg(test)]
#[path = "broker_reconcile_tests.rs"]
mod broker_reconcile_tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerOutcome {
    Accepted,
    AcceptedReconciled,
    ToolFailedRecorded,
    EvidenceMissingRecorded,
    EvidenceRecordFailed(String),
    DaemonRejected(String),
    VerifiedPendingAck,
    Uncertain(String),
}

impl BrokerOutcome {
    pub fn token(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AcceptedReconciled => "accepted_reconciled",
            Self::ToolFailedRecorded => "tool_failed_recorded",
            Self::EvidenceMissingRecorded => "evidence_missing_recorded",
            Self::EvidenceRecordFailed(_) => "evidence_record_failed",
            Self::DaemonRejected(_) => "daemon_rejected",
            Self::VerifiedPendingAck => "verified_pending_ack",
            Self::Uncertain(_) => "uncertain",
        }
    }

    pub fn outcome_line(&self) -> String {
        match self.detail() {
            Some(detail) if !detail.is_empty() => {
                format!(
                    "{OUTCOME_PREFIX}{} {}",
                    self.token(),
                    sanitize_detail(detail)
                )
            }
            _ => format!("{OUTCOME_PREFIX}{}", self.token()),
        }
    }

    fn detail(&self) -> Option<&str> {
        match self {
            Self::EvidenceRecordFailed(detail)
            | Self::DaemonRejected(detail)
            | Self::Uncertain(detail) => Some(detail),
            _ => None,
        }
    }
}

pub trait CompletionTransport {
    fn record(
        &self,
        session: &Session,
        stage: &Stage,
        evidence: &CompletionAttemptEvidence,
    ) -> Result<RecordRoute>;

    fn complete(&self, completion_nonce: &str, evidence_nonce: &str) -> Result<Response>;
}

pub struct ProductionTransport<'a> {
    stage_id: &'a str,
    session_id: &'a str,
    work_dir: &'a Path,
    complete_request: fn(&str, &str, &str, &str, &Path) -> Result<Response>,
}

impl<'a> ProductionTransport<'a> {
    pub fn new(
        stage_id: &'a str,
        session_id: &'a str,
        work_dir: &'a Path,
        complete_request: fn(&str, &str, &str, &str, &Path) -> Result<Response>,
    ) -> Self {
        Self {
            stage_id,
            session_id,
            work_dir,
            complete_request,
        }
    }
}

impl CompletionTransport for ProductionTransport<'_> {
    fn record(
        &self,
        session: &Session,
        stage: &Stage,
        evidence: &CompletionAttemptEvidence,
    ) -> Result<RecordRoute> {
        record_evidence(session, stage, evidence, self.work_dir)
    }

    fn complete(&self, completion_nonce: &str, evidence_nonce: &str) -> Result<Response> {
        (self.complete_request)(
            self.stage_id,
            self.session_id,
            completion_nonce,
            evidence_nonce,
            self.work_dir,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_broker(
    stage: &Stage,
    session: &Session,
    work_dir: &Path,
    repo_root: &Path,
    tool_status_failed: bool,
    output: &str,
    transport: &dyn CompletionTransport,
) -> BrokerOutcome {
    let (commit, exact_command) = match broker_identity(stage, repo_root) {
        Ok(identity) => identity,
        Err(error) => return BrokerOutcome::Uncertain(error.to_string()),
    };
    if tool_status_failed {
        let diagnostic = output.lines().find(|line| !line.trim().is_empty());
        return record_diagnostic(
            stage,
            session,
            commit,
            exact_command,
            CompletionPhase::ToolFailed,
            diagnostic,
            BrokerOutcome::ToolFailedRecorded,
            transport,
        );
    }

    let evidence = match parse_verified(output, stage, session, &commit, &exact_command) {
        Ok(evidence) => evidence,
        Err(error) => {
            return record_diagnostic(
                stage,
                session,
                commit,
                exact_command,
                CompletionPhase::EvidenceMissing,
                Some(&error.to_string()),
                BrokerOutcome::EvidenceMissingRecorded,
                transport,
            );
        }
    };

    if let Err(error) = transport.record(session, stage, &evidence) {
        return BrokerOutcome::EvidenceRecordFailed(error.to_string());
    }
    complete_verified(stage, session, work_dir, evidence, transport)
}

fn parse_verified(
    output: &str,
    stage: &Stage,
    session: &Session,
    commit: &str,
    exact_command: &str,
) -> Result<CompletionAttemptEvidence> {
    let evidence = parse_evidence_record(output)?;
    validate_against(&evidence, stage, &session.id, commit, exact_command)?;
    Ok(evidence)
}

fn broker_identity(stage: &Stage, repo_root: &Path) -> Result<(String, String)> {
    let commit = crate::handoff::expected_stage_commit(stage, repo_root)?;
    let executable = std::env::current_exe()?;
    Ok((commit, pinned_command(&executable, &stage.id)))
}

#[allow(clippy::too_many_arguments)]
fn record_diagnostic(
    stage: &Stage,
    session: &Session,
    commit: String,
    exact_command: String,
    phase: CompletionPhase,
    diagnostic: Option<&str>,
    success: BrokerOutcome,
    transport: &dyn CompletionTransport,
) -> BrokerOutcome {
    let evidence =
        diagnostic_evidence(stage, &session.id, commit, exact_command, phase, diagnostic);
    match transport.record(session, stage, &evidence) {
        Ok(_) => success,
        Err(error) => BrokerOutcome::EvidenceRecordFailed(error.to_string()),
    }
}

fn complete_verified(
    stage: &Stage,
    session: &Session,
    work_dir: &Path,
    evidence: CompletionAttemptEvidence,
    transport: &dyn CompletionTransport,
) -> BrokerOutcome {
    let completion_nonce = distinct_nonce(&evidence.evidence_nonce);
    match transport.complete(&completion_nonce, &evidence.evidence_nonce) {
        Ok(Response::Ok) => BrokerOutcome::Accepted,
        Ok(Response::Error { message }) => {
            record_rejection(stage, session, work_dir, &evidence, &message, transport)
        }
        Ok(other) => BrokerOutcome::Uncertain(format!("unexpected daemon response: {other:?}")),
        Err(error) => reconcile_transport_error(
            stage,
            session,
            work_dir,
            &evidence,
            &completion_nonce,
            &error.to_string(),
            transport,
        ),
    }
}

fn distinct_nonce(evidence_nonce: &str) -> String {
    loop {
        let nonce = fresh_nonce();
        if nonce != evidence_nonce {
            return nonce;
        }
    }
}

fn record_rejection(
    stage: &Stage,
    session: &Session,
    work_dir: &Path,
    evidence: &CompletionAttemptEvidence,
    message: &str,
    transport: &dyn CompletionTransport,
) -> BrokerOutcome {
    match load_stage(&stage.id, work_dir) {
        Ok(durable) if durable.status == StageStatus::Completed => {}
        Ok(_) => {
            let rejected = with_boundary_failure(
                evidence,
                CompletionPhase::DaemonRejected,
                "daemon_rejected",
                Some(message),
            );
            if let Err(error) = transport.record(session, stage, &rejected) {
                return BrokerOutcome::EvidenceRecordFailed(error.to_string());
            }
        }
        Err(error) => return BrokerOutcome::Uncertain(error.to_string()),
    }
    BrokerOutcome::DaemonRejected(message.to_string())
}

#[allow(clippy::too_many_arguments)]
fn reconcile_transport_error(
    stage: &Stage,
    session: &Session,
    work_dir: &Path,
    evidence: &CompletionAttemptEvidence,
    completion_nonce: &str,
    error: &str,
    transport: &dyn CompletionTransport,
) -> BrokerOutcome {
    let durable = match load_stage(&stage.id, work_dir) {
        Ok(durable) => durable,
        Err(load_error) => return BrokerOutcome::Uncertain(load_error.to_string()),
    };
    if durable.status == StageStatus::Completed {
        return reconcile_completed(stage, session, work_dir, evidence, completion_nonce);
    }
    if durable.status != StageStatus::Executing {
        return BrokerOutcome::Uncertain(format!(
            "daemon transport failed while stage is {}",
            durable.status
        ));
    }
    let pending = with_boundary_failure(
        evidence,
        CompletionPhase::VerifiedPendingAck,
        "daemon_transport",
        Some(error),
    );
    match transport.record(session, stage, &pending) {
        Ok(_) => BrokerOutcome::VerifiedPendingAck,
        Err(record_error) => BrokerOutcome::EvidenceRecordFailed(record_error.to_string()),
    }
}

fn reconcile_completed(
    stage: &Stage,
    session: &Session,
    work_dir: &Path,
    evidence: &CompletionAttemptEvidence,
    completion_nonce: &str,
) -> BrokerOutcome {
    let checkpoint = match load_trusted_session_checkpoint(&stage.id, &session.id, work_dir) {
        Ok(Some(checkpoint)) => checkpoint,
        Ok(None) => return BrokerOutcome::Uncertain("accepted receipt is missing".to_string()),
        Err(error) => return BrokerOutcome::Uncertain(error.to_string()),
    };
    let matches = checkpoint.accepted.as_ref().is_some_and(|receipt| {
        receipt.evidence_nonce == evidence.evidence_nonce
            && receipt.completion_nonce == completion_nonce
    });
    if matches {
        BrokerOutcome::AcceptedReconciled
    } else {
        BrokerOutcome::Uncertain("accepted receipt does not match completion attempt".to_string())
    }
}

fn sanitize_detail(detail: &str) -> String {
    detail
        .chars()
        .filter(|character| !character.is_control())
        .take(300)
        .collect()
}
