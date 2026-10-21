//! The handlers a drained request reaches: the daemon's existing code paths,
//! each answering with a [`Settle`]. A handler that fails outright settles as
//! a refusal naming the failure: `applying` is already recorded, so the
//! request is never retried. A relayed commit goes to [`super::commit`], which
//! maps the session to the branch it may commit and commits through
//! `git::stage_commit`.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::daemon::{
    handle_agent_block_stage, handle_dispute_criteria, handle_file_dispute,
    handle_freeze_contracts, Response,
};
use crate::fs::memory::{append_entry, validate_spooled_entry, MemoryEntry};
use crate::fs::stage_request::StageRequest;
use crate::handoff::session_content::{write_session_handoff, SessionHandoff, CEILING_TRIGGER};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::adjudication::record::{self, AdjudicateOutcome};
use crate::relay::{
    decode_payload, verdict as matrix_verdict, AgentRole, CommitPayload, HandoffRequest,
    InboxEntry, MatrixVerdict, RequestPayload, VerdictRequest,
};
use crate::telemetry::{append_record, TelemetryEvent, TelemetryRecord};
use crate::verify::transitions::{load_stage, update_stage};

use super::commit::{self, CommitSite};
use super::{check, complete, freeze};
use super::{InboxHost, Settle};

/// An entry the matrix lets through, with its payload decoded.
pub(super) struct Admitted {
    request_id: String,
    stage_id: String,
    relayed_at: DateTime<Utc>,
    matrix: MatrixVerdict,
    payload: RequestPayload,
}

/// The writer, the matrix row for the record's session kind, then the payload
/// shape. A control request from a subagent is refused here as well as in the
/// relay hook, so an entry written past the hook still never acts with the
/// main agent's authority.
pub(super) fn admit(record: &Session, inbox_entry: &InboxEntry) -> Result<Admitted, String> {
    if inbox_entry.agent == AgentRole::Subagent && inbox_entry.kind.is_control() {
        return Err(format!(
            "a '{}' request from a subagent is never applied: only the session's main agent \
             makes it",
            inbox_entry.kind
        ));
    }
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
        request_id: inbox_entry.id.clone(),
        stage_id: inbox_entry.stage_id.clone(),
        relayed_at: inbox_entry.relayed_at,
        matrix,
        payload,
    })
}

/// Hand one admitted request to the handler that owns its kind.
pub(super) fn apply(host: &mut dyn InboxHost, record: &Session, admitted: Admitted) -> Settle {
    let (work_dir, repo_root) = (
        host.work_dir().to_path_buf(),
        host.repo_root().to_path_buf(),
    );
    let (stage_id, request_id) = (admitted.stage_id.as_str(), admitted.request_id.as_str());
    let site = RequestSite {
        work_dir: &work_dir,
        repo_root: &repo_root,
        stage_id,
        record,
    };
    match admitted.payload {
        RequestPayload::Memory(entry) => memory(&work_dir, stage_id, &entry),
        // A resolver is not its stage's worker, so its block is a give-up.
        RequestPayload::Block(StageRequest::Block { reason })
            if record.session_type == SessionType::Merge =>
        {
            host.give_up_merge(record, stage_id, &reason)
        }
        RequestPayload::Block(request)
        | RequestPayload::Dispute(request)
        | RequestPayload::FreezeContracts(request)
        | RequestPayload::FileDispute(request) => {
            stage_request(&work_dir, stage_id, record, request, request_id)
        }
        RequestPayload::Handoff(request) => handoff(&site, &request, admitted.matrix),
        RequestPayload::MergeResolved => host.resolve_merge(record, stage_id),
        RequestPayload::Verdict(request) => {
            adjudication_verdict(&*host, stage_id, record, &request)
        }
        RequestPayload::Telemetry(event) => {
            telemetry(&work_dir, stage_id, record, event, admitted.relayed_at)
        }
        RequestPayload::Commit(payload) => commit_request(host, &site, &payload),
        RequestPayload::Check(request) => {
            check::apply_check(host, stage_id, record, &request, request_id)
        }
        RequestPayload::Complete(request) => {
            complete::apply_complete(host, stage_id, record, &request, request_id)
        }
    }
}

/// A relayed commit, applied by `commit` from the request's site.
fn commit_request(
    host: &mut dyn InboxHost,
    site: &RequestSite<'_>,
    payload: &CommitPayload,
) -> Settle {
    let commit_site = CommitSite {
        work_dir: site.work_dir,
        repo_root: site.repo_root,
        stage_id: site.stage_id,
        record: site.record,
    };
    commit::apply_commit(host, &commit_site, payload)
}

pub(super) fn failed(error: anyhow::Error) -> Settle {
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
    request_id: &str,
) -> Settle {
    if let Err(reason) = require_owner(work_dir, stage_id, record) {
        return Settle::Refused(reason);
    }
    let response = match request {
        StageRequest::Block { reason } => handle_agent_block_stage(work_dir, stage_id, &reason),
        StageRequest::Dispute {
            field,
            criterion_indices,
            reason,
            evidence_commit,
            failure_output,
        } => handle_dispute_criteria(
            work_dir,
            stage_id,
            field,
            criterion_indices,
            reason,
            evidence_commit,
            failure_output,
        ),
        StageRequest::FreezeContracts { head, tree_digest } => {
            let queued = handle_freeze_contracts(
                work_dir,
                stage_id,
                &record.id,
                &head,
                &tree_digest,
                request_id,
            );
            return freeze::settle(work_dir, stage_id, &record.id, request_id, queued);
        }
        StageRequest::FileDispute {
            kind,
            reason,
            evidence_commit,
        } => handle_file_dispute(work_dir, stage_id, kind, reason, evidence_commit),
    };
    settle_response(response)
}

/// Settles a stage handler's daemon answer: an error reply refuses the request
/// with its message, and a handler failure refuses it with the error.
fn settle_response(response: anyhow::Result<Response>) -> Settle {
    match response {
        Ok(Response::Ok) => Settle::Applied(None),
        Ok(Response::DisputeCreated { id }) => Settle::Applied(Some(format!("dispute {id} filed"))),
        Ok(Response::Error { message }) => Settle::Refused(message),
        Ok(other) => Settle::Refused(format!("unexpected daemon answer: {other:?}")),
        Err(error) => failed(error),
    }
}

/// Block, disputes, freeze and a stage or knowledge commit act on a stage only
/// for the live session that owns it, the rule `daemon::server` enforces for
/// the same requests over the socket. The freeze handler further requires a
/// contract session. Returns the stage as loaded for the check.
pub(super) fn require_owner(
    work_dir: &Path,
    stage_id: &str,
    record: &Session,
) -> Result<Stage, String> {
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
    Ok(stage)
}

/// Where a relayed handoff or commit is applied from.
struct RequestSite<'a> {
    work_dir: &'a Path,
    repo_root: &'a Path,
    stage_id: &'a str,
    record: &'a Session,
}

fn handoff(site: &RequestSite<'_>, request: &HandoffRequest, matrix: MatrixVerdict) -> Settle {
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

/// Whether `record` is a running adjudication session whose process is
/// alive: the only session a judge's verdict or `loom check` is taken from.
pub(super) fn is_live_judge(host: &dyn InboxHost, record: &Session) -> bool {
    record.session_type == SessionType::Adjudication
        && record.status == SessionStatus::Running
        && matches!(host.session_alive(record), Ok(true))
}

/// A judge's verdict, recorded only for the live adjudication session of the
/// stage and under the recording path's own guards.
fn adjudication_verdict(
    host: &dyn InboxHost,
    stage_id: &str,
    record: &Session,
    request: &VerdictRequest,
) -> Settle {
    if !is_live_judge(host, record) {
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
