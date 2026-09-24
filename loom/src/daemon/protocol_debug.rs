//! `Debug` for [`Request`]: every credential and every agent-written text is
//! redacted, so a request can be logged.

use std::fmt;

use super::Request;

/// Render a request whose only field is its credential: name it, redact that.
fn credential_only(formatter: &mut fmt::Formatter<'_>, name: &str) -> fmt::Result {
    write!(formatter, "{name} {{ auth_token: [REDACTED] }}")
}

fn debug_dispute(request: &Request, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Request::DisputeCriteria {
        stage_id,
        session_id,
        criterion_index,
        evidence_commit,
        ..
    } = request
    else {
        unreachable!("debug_dispute called for another request variant")
    };
    formatter
        .debug_struct("DisputeCriteria")
        .field("auth_token", &"[REDACTED]")
        .field("stage_id", stage_id)
        .field("session_id", session_id)
        .field("criterion_index", criterion_index)
        .field("reason", &"[REDACTED]")
        .field("evidence_commit", evidence_commit)
        .field("failure_output", &"[REDACTED]")
        .finish()
}

/// The disputed ids and their evidence are agent-written, so only the kind is
/// shown.
fn debug_file_dispute(request: &Request, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Request::FileDispute {
        stage_id,
        session_id,
        kind,
        evidence_commit,
        ..
    } = request
    else {
        unreachable!("debug_file_dispute called for another request variant")
    };
    formatter
        .debug_struct("FileDispute")
        .field("auth_token", &"[REDACTED]")
        .field("stage_id", stage_id)
        .field("session_id", session_id)
        .field("kind", &kind.name())
        .field("reason", &"[REDACTED]")
        .field("evidence_commit", evidence_commit)
        .finish()
}

/// Contract ids are agent-written, so only the number of reports is shown.
fn debug_freeze(request: &Request, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Request::FreezeContracts {
        stage_id,
        session_id,
        reports,
        ..
    } = request
    else {
        unreachable!("debug_freeze called for another request variant")
    };
    formatter
        .debug_struct("FreezeContracts")
        .field("auth_token", &"[REDACTED]")
        .field("stage_id", stage_id)
        .field("session_id", session_id)
        .field("reports", &reports.len())
        .finish()
}

fn debug_completion(request: &Request, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match request {
        Request::CompleteStage {
            stage_id,
            session_id,
            nonce,
            evidence_nonce,
            ..
        } => formatter
            .debug_struct("CompleteStage")
            .field("auth_token", &"[REDACTED]")
            .field("stage_id", stage_id)
            .field("session_id", session_id)
            .field("nonce", nonce)
            .field("evidence_nonce", evidence_nonce)
            .finish(),
        Request::RecordCompletionEvidence {
            stage_id,
            session_id,
            evidence,
            ..
        } => formatter
            .debug_struct("RecordCompletionEvidence")
            .field("auth_token", &"[REDACTED]")
            .field("stage_id", stage_id)
            .field("session_id", session_id)
            .field("phase", &evidence.phase)
            .field("evidence_nonce", &evidence.evidence_nonce)
            .finish(),
        _ => unreachable!("debug_completion called for another request variant"),
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Request::SubscribeStatus { .. } => credential_only(formatter, "SubscribeStatus"),
            Request::SubscribeLogs { .. } => credential_only(formatter, "SubscribeLogs"),
            Request::Stop { .. } => credential_only(formatter, "Stop"),
            Request::Unsubscribe { .. } => credential_only(formatter, "Unsubscribe"),
            Request::Ping { .. } => credential_only(formatter, "Ping"),
            Request::DisputeCriteria { .. } => debug_dispute(self, formatter),
            Request::FileDispute { .. } => debug_file_dispute(self, formatter),
            Request::BlockStage {
                stage_id,
                session_id,
                ..
            } => formatter
                .debug_struct("BlockStage")
                .field("auth_token", &"[REDACTED]")
                .field("stage_id", stage_id)
                .field("session_id", session_id)
                .field("reason", &"[REDACTED]")
                .finish(),
            Request::CompleteStage { .. } | Request::RecordCompletionEvidence { .. } => {
                debug_completion(self, formatter)
            }
            Request::FreezeContracts { .. } => debug_freeze(self, formatter),
        }
    }
}
