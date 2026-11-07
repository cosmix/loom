//! Stage-control dispatch after relay ownership checks.

use super::super::{freeze, Settle};
use crate::daemon::{handle_freeze_contracts, Response};
use crate::fs::stage_request::StageRequest;
use crate::models::session::Session;
use crate::plan::schema::ContractChoices;
use std::path::Path;

pub(super) fn dispatch_stage_request(
    work_dir: &Path,
    stage_id: &str,
    record: &Session,
    request: StageRequest,
    request_id: &str,
) -> anyhow::Result<Response> {
    match request {
        StageRequest::Block { reason } => crate::stage_service::handle_agent_block_for_session(
            work_dir,
            stage_id,
            &reason,
            Some(&record.id),
        )
        .map(Into::into),
        StageRequest::Dispute {
            field,
            criterion_indices,
            reason,
            evidence_commit,
            failure_output,
        } => crate::stage_service::handle_dispute_criteria_for_session(
            work_dir,
            stage_id,
            (field, criterion_indices),
            reason,
            evidence_commit,
            failure_output,
            (Some(request_id), Some(&record.id)),
        )
        .map(Into::into),
        StageRequest::FreezeContracts { .. } => unreachable!("freeze settled by caller"),
        StageRequest::FileDispute {
            kind,
            reason,
            evidence_commit,
        } => crate::stage_service::handle_file_dispute_for_session(
            work_dir,
            stage_id,
            kind,
            reason,
            evidence_commit,
            (Some(request_id), Some(&record.id)),
        )
        .map(Into::into),
    }
}

pub(super) fn settle_freeze(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    request_id: &str,
    (head, tree_digest): (&str, &str),
    choices: &ContractChoices,
) -> Settle {
    let queued = handle_freeze_contracts(
        work_dir,
        stage_id,
        session_id,
        head,
        tree_digest,
        request_id,
        choices,
    );
    freeze::settle(work_dir, stage_id, session_id, request_id, queued)
}
