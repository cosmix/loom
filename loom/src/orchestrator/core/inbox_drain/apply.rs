//! The handlers a drained request reaches: the daemon's existing code paths,
//! each answering with a [`Settle`]. A handler that fails outright settles as
//! a refusal naming the failure: `applying` is already recorded, so the
//! request is never retried.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::daemon::{
    handle_block_stage, handle_dispute_criteria, handle_freeze_contracts, Response,
};
use crate::fs::memory::{append_entry, validate_spooled_entry, MemoryEntry};
use crate::fs::stage_request::StageRequest;
use crate::handoff::session_content::{write_session_handoff, SessionHandoff, CEILING_TRIGGER};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::orchestrator::adjudication::record::{self, AdjudicateOutcome};
use crate::relay::{
    decode_payload, verdict as matrix_verdict, HandoffRequest, InboxEntry, MatrixVerdict,
    RequestPayload, VerdictRequest,
};
use crate::telemetry::{append_record, TelemetryEvent, TelemetryRecord};
use crate::verify::transitions::{load_stage, update_stage};

use super::{InboxHost, Settle};

/// An entry the matrix lets through, with its payload decoded.
pub(super) struct Admitted {
    stage_id: String,
    relayed_at: DateTime<Utc>,
    matrix: MatrixVerdict,
    payload: RequestPayload,
}

/// The matrix row for the record's session kind, then the payload shape.
pub(super) fn admit(record: &Session, inbox_entry: &InboxEntry) -> Result<Admitted, String> {
    let matrix = matrix_verdict(record.session_type, inbox_entry.kind);
    if matrix == MatrixVerdict::Refuse {
        return Err(format!(
            "a {} session may not relay a '{}' request",
            record.session_type, inbox_entry.kind
        ));
    }
    let payload =
        decode_payload(inbox_entry.kind, &inbox_entry.payload).map_err(|e| format!("{e:#}"))?;
    Ok(Admitted {
        stage_id: inbox_entry.stage_id.clone(),
        relayed_at: inbox_entry.relayed_at,
        matrix,
        payload,
    })
}

/// Hand one admitted request to the handler that owns its kind.
pub(super) fn apply(host: &mut dyn InboxHost, record: &Session, admitted: Admitted) -> Settle {
    let work_dir = host.work_dir().to_path_buf();
    let stage_id = admitted.stage_id.as_str();
    match admitted.payload {
        RequestPayload::Memory(entry) => memory(&work_dir, stage_id, &entry),
        RequestPayload::Block(request)
        | RequestPayload::Dispute(request)
        | RequestPayload::FreezeContracts(request) => {
            stage_request(&work_dir, stage_id, record, request)
        }
        RequestPayload::Handoff(request) => {
            let repo_root = host.repo_root().to_path_buf();
            let site = HandoffSite {
                work_dir: &work_dir,
                repo_root: &repo_root,
                stage_id,
                record,
            };
            handoff(&site, &request, admitted.matrix)
        }
        RequestPayload::MergeResolved => host.resolve_merge(record, stage_id),
        RequestPayload::Verdict(request) => {
            adjudication_verdict(&*host, stage_id, record, &request)
        }
        RequestPayload::Telemetry(event) => {
            telemetry(&work_dir, stage_id, record, event, admitted.relayed_at)
        }
    }
}

fn failed(error: anyhow::Error) -> Settle {
    Settle::Refused(format!("could not be applied: {error:#}"))
}

fn memory(work_dir: &Path, stage_id: &str, entry: &MemoryEntry) -> Settle {
    if let Err(error) = validate_spooled_entry(entry) {
        return Settle::Refused(format!("memory entry failed validation: {error:#}"));
    }
    match append_entry(work_dir, stage_id, entry) {
        Ok(()) => Settle::Applied(None),
        Err(error) => failed(error),
    }
}

fn stage_request(
    work_dir: &Path,
    stage_id: &str,
    record: &Session,
    request: StageRequest,
) -> Settle {
    if let Err(reason) = require_owner(work_dir, stage_id, record) {
        return Settle::Refused(reason);
    }
    let response = match request {
        StageRequest::Block { reason } => handle_block_stage(work_dir, stage_id, &reason),
        StageRequest::Dispute {
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
        } => handle_dispute_criteria(
            work_dir,
            stage_id,
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
        ),
        StageRequest::FreezeContracts { reports } => {
            handle_freeze_contracts(work_dir, stage_id, &record.id, &reports)
        }
    };
    match response {
        Ok(Response::Ok) => Settle::Applied(None),
        Ok(Response::DisputeCreated { id }) => Settle::Applied(Some(format!("dispute {id} filed"))),
        Ok(Response::ContractsFrozen { files }) => {
            Settle::Applied(Some(format!("contracts frozen ({files} files)")))
        }
        Ok(Response::Error { message }) => Settle::Refused(message),
        Ok(other) => Settle::Refused(format!("unexpected daemon answer: {other:?}")),
        Err(error) => failed(error),
    }
}

/// Block, dispute and freeze act on a stage only for the live session that
/// owns it, the rule `daemon::server` enforces for the same requests over the
/// socket. The freeze handler further requires a contract session.
fn require_owner(work_dir: &Path, stage_id: &str, record: &Session) -> Result<(), String> {
    if record.status != SessionStatus::Running {
        return Err(format!("session '{}' is no longer running", record.id));
    }
    let stage = load_stage(stage_id, work_dir)
        .map_err(|error| format!("stage '{stage_id}' could not be loaded: {error:#}"))?;
    if stage.session.as_deref() != Some(record.id.as_str()) {
        return Err(format!(
            "session '{}' is not the active session of stage '{stage_id}'",
            record.id
        ));
    }
    Ok(())
}

/// Where a relayed handoff is written from.
struct HandoffSite<'a> {
    work_dir: &'a Path,
    repo_root: &'a Path,
    stage_id: &'a str,
    record: &'a Session,
}

fn handoff(site: &HandoffSite<'_>, request: &HandoffRequest, matrix: MatrixVerdict) -> Settle {
    let stage = match load_stage(site.stage_id, site.work_dir) {
        Ok(stage) => stage,
        Err(error) => return failed(error),
    };
    let ends_turn = matrix == MatrixVerdict::Apply && request.trigger == CEILING_TRIGGER;
    let checkout = site
        .record
        .worktree_path
        .clone()
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| site.repo_root.to_path_buf());
    let document = SessionHandoff {
        session: site.record,
        stage: &stage,
        checkout: &checkout,
        trigger: &request.trigger,
        message: request.message.as_deref(),
        ends_turn,
    };
    let path = match write_session_handoff(site.work_dir, &document) {
        Ok(path) => path,
        Err(error) => return failed(error),
    };
    let written = format!("handoff written to {}", path.display());
    if !ends_turn {
        return Settle::Applied(Some(written));
    }
    let transition = end_turn(site.work_dir, site.stage_id, &site.record.id);
    Settle::Applied(Some(format!("{written}; {transition}")))
}

/// With `--trigger ceiling`, take the stage to `NeedsHandoff` so the daemon
/// ends the session and re-queues the stage — only while it is `Executing`
/// and still this session's.
fn end_turn(work_dir: &Path, stage_id: &str, session_id: &str) -> String {
    let mut left = None;
    let result = update_stage(stage_id, work_dir, |stage| {
        if stage.status != StageStatus::Executing || stage.session.as_deref() != Some(session_id) {
            left = Some(stage.status.clone());
            return Ok(());
        }
        stage.try_mark_needs_handoff()
    });
    match (result, left) {
        (Err(error), _) => format!("marking the stage NeedsHandoff failed: {error:#}"),
        (Ok(_), Some(status)) => format!("stage left {status}: not Executing under this session"),
        (Ok(_), None) => "stage marked NeedsHandoff".to_string(),
    }
}

/// A judge's verdict, recorded only for the live adjudication session of the
/// stage and under the recording path's own guards.
fn adjudication_verdict(
    host: &dyn InboxHost,
    stage_id: &str,
    record: &Session,
    request: &VerdictRequest,
) -> Settle {
    let live = record.session_type == SessionType::Adjudication
        && record.status == SessionStatus::Running
        && matches!(host.session_alive(record), Ok(true));
    if !live {
        return Settle::Refused(format!(
            "only the live adjudication session for stage '{stage_id}' may record its verdict"
        ));
    }
    let worktree = record
        .worktree_path
        .as_deref()
        .map(|path| path.to_string_lossy().into_owned());
    if let Err(error) = record::refuse_worktree_session(worktree.as_deref()) {
        return Settle::Refused(format!("{error:#}"));
    }
    let session_id = Some(record.id.clone());
    let dispute_id = request.dispute_id;
    match record::record_verdict_text(
        host.work_dir(),
        stage_id,
        dispute_id,
        &request.verdict,
        session_id,
    ) {
        Ok(AdjudicateOutcome::Recorded) => {
            Settle::Applied(Some(format!("verdict for dispute {dispute_id} recorded")))
        }
        Ok(AdjudicateOutcome::Escalated(reason)) => Settle::Applied(Some(format!(
            "degenerate verdict; stage escalated to NeedsHumanReview: {reason}"
        ))),
        Err(error) => Settle::Refused(format!("{error:#}")),
    }
}

fn telemetry(
    work_dir: &Path,
    stage_id: &str,
    record: &Session,
    event: TelemetryEvent,
    at: DateTime<Utc>,
) -> Settle {
    let telemetry_record = TelemetryRecord {
        at,
        event: attribute(event, stage_id, &record.id),
    };
    match append_record(work_dir, &telemetry_record) {
        Ok(()) => Settle::Applied(None),
        Err(error) => failed(error),
    }
}

/// Attribution comes from the session record, not from what the ticket said.
fn attribute(event: TelemetryEvent, stage_id: &str, session_id: &str) -> TelemetryEvent {
    match event {
        TelemetryEvent::ContextPulled {
            query_chars,
            budget_tokens,
            items,
            estimated_tokens,
            unmet_required,
            ..
        } => TelemetryEvent::ContextPulled {
            stage_id: Some(stage_id.to_string()),
            session_id: Some(session_id.to_string()),
            query_chars,
            budget_tokens,
            items,
            estimated_tokens,
            unmet_required,
        },
        other => other,
    }
}
