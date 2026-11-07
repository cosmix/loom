//! Applying drained requests through the shared stage service.
//!
//! Nothing here reimplements a state transition. A spooled block goes through
//! the same `handle_agent_block_for_session` an agent's `BlockStage` RPC calls, a spooled
//! dispute through the same `handle_dispute_criteria` or `handle_file_dispute`
//! its RPC calls, and a spooled contract freeze through the same
//! `handle_freeze_contracts` the `FreezeContracts` RPC calls (which queues the
//! host contract run), so the two ways a request can arrive cannot disagree
//! about what it does - including the refusals, the budget check, the id
//! allocation and the `validate_id` guard on the stage id.

use anyhow::Result;
use std::path::Path;

use super::spool::{drain_identified, DrainOutcome};
use super::types::StageRequest;
use crate::plan::schema::ContractChoices;
use crate::stage_service::{
    handle_agent_block_for_session, handle_dispute_criteria_for_session,
    handle_file_dispute_for_session, handle_freeze_contracts, Response,
};

/// Apply every request pending in `worktree_root`'s spool to `stage_id`, then
/// empty the spool.
///
/// `stage_id` comes from the caller (the daemon, which derived it from the
/// worktree it is looking at), never from the payload - see the module
/// documentation. The envelope's session must still own that stage.
///
/// A refusal from a handler is COUNTED, not propagated: `Err` from this sink
/// would stop `drain_spool` truncating the file, so one request the stage
/// can never take - a block of an already-completed stage, a dispute past its
/// budget - would redeliver forever, wedged in front of every request behind
/// it. `Err` is reserved for a genuine I/O failure from a handler, where
/// blocking the truncate and retrying next tick is the correct behavior.
pub fn drain_requests(
    work_dir: &Path,
    stage_id: &str,
    worktree_root: &Path,
) -> Result<DrainOutcome> {
    let mut refused = 0usize;
    let outcome = drain_identified(worktree_root, &mut |request| {
        apply_request(
            work_dir,
            stage_id,
            &request.payload,
            &request.identity(),
            request.session_id.as_deref(),
            &mut refused,
        )
    })?;
    Ok(DrainOutcome {
        applied: outcome.applied.saturating_sub(refused),
        skipped: outcome.skipped + refused,
    })
}

/// Hand one request to the daemon handler that owns it, and account for the
/// answer.
fn apply_request(
    work_dir: &Path,
    stage_id: &str,
    request: &StageRequest,
    request_id: &str,
    session_id: Option<&str>,
    refused: &mut usize,
) -> Result<()> {
    let Some(session_id) = session_id.filter(|id| !id.is_empty()) else {
        *refused += 1;
        tracing::warn!(stage_id, "discarding unscoped legacy stage request");
        return Ok(());
    };
    let response = dispatch_request(work_dir, stage_id, request, request_id, session_id)?;
    record_response(stage_id, request, response, refused);
    Ok(())
}

fn dispatch_request(
    work_dir: &Path,
    stage_id: &str,
    request: &StageRequest,
    request_id: &str,
    session_id: &str,
) -> Result<Response> {
    let ids = (session_id, request_id);
    Ok(match request {
        StageRequest::Block { reason } => {
            handle_agent_block_for_session(work_dir, stage_id, reason, Some(session_id))?
        }
        StageRequest::Dispute {
            field,
            criterion_indices,
            reason,
            evidence_commit,
            failure_output,
        } => handle_dispute_criteria_for_session(
            work_dir,
            stage_id,
            (*field, criterion_indices.clone()),
            reason.clone(),
            evidence_commit.clone(),
            failure_output.clone(),
            (Some(request_id), Some(session_id)),
        )?,
        StageRequest::FreezeContracts {
            head,
            tree_digest,
            choices,
        } => dispatch_freeze(work_dir, stage_id, ids, (head, tree_digest), choices)?,
        StageRequest::FileDispute {
            kind,
            reason,
            evidence_commit,
        } => handle_file_dispute_for_session(
            work_dir,
            stage_id,
            kind.clone(),
            reason.clone(),
            evidence_commit.clone(),
            (Some(request_id), Some(session_id)),
        )?,
    })
}

fn dispatch_freeze(
    work_dir: &Path,
    stage_id: &str,
    (session_id, request_id): (&str, &str),
    (head, tree_digest): (&str, &str),
    choices: &ContractChoices,
) -> Result<Response> {
    // An unreadable stage is retryable rather than a permanent refusal.
    crate::verify::transitions::load_stage(stage_id, work_dir)?;
    handle_freeze_contracts(
        work_dir,
        stage_id,
        session_id,
        head,
        tree_digest,
        request_id,
        choices,
    )
}

/// Log what the daemon did with one drained request, and count the refusals.
///
/// Logged at `info` because without this line an operator watching `loom status`
/// sees a stage go `Blocked` with nothing in the log that says why.
fn record_response(
    stage_id: &str,
    request: &StageRequest,
    response: Response,
    refused: &mut usize,
) {
    match response {
        Response::Ok => tracing::info!(
            stage_id = %stage_id,
            request = request.kind(),
            "Applied a spooled stage request"
        ),
        Response::DisputeCreated { id } => tracing::info!(
            stage_id = %stage_id,
            dispute_id = id,
            "Filed a spooled dispute"
        ),
        Response::ContractsFrozen { files } => tracing::info!(
            stage_id = %stage_id,
            files,
            "Froze a spooled contract set"
        ),
        Response::Error { message } => {
            *refused += 1;
            tracing::warn!(
                stage_id = %stage_id,
                request = request.kind(),
                reason = %message,
                "Refused a spooled stage request; it is discarded, not retried"
            );
        }
    }
}

#[cfg(test)]
#[path = "tests/apply.rs"]
mod tests;
