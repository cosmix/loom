//! Applying drained requests through the shared stage service.
//!
//! Nothing here reimplements a state transition. A spooled block goes through
//! the same `handle_agent_block_stage` an agent's `BlockStage` RPC calls, a spooled
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
use crate::stage_service::{
    handle_agent_block_stage, handle_dispute_criteria_identified, handle_file_dispute_identified,
    handle_freeze_contracts, Response,
};
use crate::verify::transitions::load_stage;

/// Apply every request pending in `worktree_root`'s spool to `stage_id`, then
/// empty the spool.
///
/// `stage_id` comes from the caller (the daemon, which derived it from the
/// worktree it is looking at), never from the payload - see the module
/// documentation for why that is the whole security argument.
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
    refused: &mut usize,
) -> Result<()> {
    let response = match request {
        StageRequest::Block { reason } => handle_agent_block_stage(work_dir, stage_id, reason)?,
        StageRequest::Dispute {
            field,
            criterion_indices,
            reason,
            evidence_commit,
            failure_output,
        } => handle_dispute_criteria_identified(
            work_dir,
            stage_id,
            (*field, criterion_indices.clone()),
            reason.clone(),
            evidence_commit.clone(),
            failure_output.clone(),
            Some(request_id),
        )?,
        StageRequest::FreezeContracts { head, tree_digest } => {
            freeze_as_current_session(work_dir, stage_id, head, tree_digest)?
        }
        StageRequest::FileDispute {
            kind,
            reason,
            evidence_commit,
        } => handle_file_dispute_identified(
            work_dir,
            stage_id,
            kind.clone(),
            reason.clone(),
            evidence_commit.clone(),
            Some(request_id),
        )?,
    };
    record_response(stage_id, request, response, refused);
    Ok(())
}

/// A spool names no session, so a spooled freeze speaks for the session the
/// stage runs now; the handler refuses it unless that is the stage's running
/// contract session. The spool line has no relay request id, so the job is
/// joined under a fresh one.
fn freeze_as_current_session(
    work_dir: &Path,
    stage_id: &str,
    head: &str,
    tree_digest: &str,
) -> Result<Response> {
    let session = load_stage(stage_id, work_dir)?.session.unwrap_or_default();
    let request_id = crate::host_verify::new_job_id();
    handle_freeze_contracts(work_dir, stage_id, &session, head, tree_digest, &request_id)
}

/// Log what the daemon did with one drained request, and count the refusals.
///
/// Logged at `info` because a stage changing state from a spool has no session
/// attached to explain it: without this line an operator watching `loom status`
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
