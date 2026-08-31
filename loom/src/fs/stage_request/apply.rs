//! Applying drained requests, through the daemon's own handlers.
//!
//! Nothing here reimplements a state transition. A spooled block goes through
//! the same `handle_block_stage` the `BlockStage` RPC calls, and a spooled
//! dispute through the same `handle_dispute_criteria` the `DisputeCriteria`
//! RPC calls, so the two ways a request can arrive cannot disagree about what
//! it does - including the refusals, the budget check, the id allocation and
//! the `validate_id` guard on the stage id.

use anyhow::Result;
use std::path::Path;

use super::spool::{drain_spool, DrainOutcome};
use super::types::StageRequest;
use crate::daemon::{handle_block_stage, handle_dispute_criteria, Response};

/// Apply every request pending in `worktree_root`'s spool to `stage_id`, then
/// empty the spool.
///
/// `stage_id` comes from the caller (the daemon, which derived it from the
/// worktree it is looking at), never from the payload - see the module
/// documentation for why that is the whole security argument.
///
/// A refusal from a handler is COUNTED, not propagated: `Err` from this sink
/// would stop [`drain_spool`] truncating the file, so one request the stage
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
    let outcome = drain_spool(worktree_root, &mut |request| {
        apply_request(work_dir, stage_id, request, &mut refused)
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
    refused: &mut usize,
) -> Result<()> {
    let response = match request {
        StageRequest::Block { reason } => handle_block_stage(work_dir, stage_id, reason)?,
        StageRequest::Dispute {
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
        } => handle_dispute_criteria(
            work_dir,
            stage_id,
            *criterion_index,
            reason.clone(),
            evidence_commit.clone(),
            failure_output.clone(),
        )?,
    };
    record_response(stage_id, request, response, refused);
    Ok(())
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
        Response::Error { message } => {
            *refused += 1;
            tracing::warn!(
                stage_id = %stage_id,
                request = request.kind(),
                reason = %message,
                "Refused a spooled stage request; it is discarded, not retried"
            );
        }
        other => {
            *refused += 1;
            tracing::warn!(
                stage_id = %stage_id,
                request = request.kind(),
                response = ?other,
                "Unexpected daemon answer to a spooled stage request; it is discarded"
            );
        }
    }
}

#[cfg(test)]
#[path = "tests/apply.rs"]
mod tests;
