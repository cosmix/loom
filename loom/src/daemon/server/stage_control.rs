//! Serving the requests a stage agent makes about its own stage — its
//! disputes, its block and its contract freeze — once `client.rs` has settled
//! authorization and stage ownership. Each handler's failure to persist comes
//! back as a `Response::Error` naming it.

use std::path::Path;

use crate::daemon::protocol::{ContractRunReport, Request, Response};
use crate::models::dispute::DisputeKind;

/// Serve one `DisputeCriteria`.
///
/// The handler owns `request.md` persistence and the transition to
/// `NeedsAdjudication`. Authorization and stage ownership are settled before
/// it runs; the handler additionally validates `criterion_index` and the
/// stage's dispute budget.
fn serve_dispute_criteria(
    work_dir: &Path,
    stage_id: &str,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
) -> Response {
    super::dispute::handle_dispute_criteria(
        work_dir,
        stage_id,
        criterion_index,
        reason,
        evidence_commit,
        failure_output,
    )
    .unwrap_or_else(|error| Response::Error {
        message: format!("Dispute persistence failed: {error:#}"),
    })
}

/// Serve one `FileDispute`. The handler checks the disputed ids and the
/// kind's budget itself; ownership was settled like the criterion dispute's.
fn serve_file_dispute(
    work_dir: &Path,
    stage_id: &str,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
) -> Response {
    super::dispute_kinds::handle_file_dispute(work_dir, stage_id, kind, reason, evidence_commit)
        .unwrap_or_else(|error| Response::Error {
            message: format!("Dispute persistence failed: {error:#}"),
        })
}

/// Serve one `BlockStage`. A transition the state machine refuses comes back
/// from the handler as `Response::Error`, not as an `Err`.
fn serve_block_stage(work_dir: &Path, stage_id: &str, reason: &str) -> Response {
    super::control_block::handle_block_stage(work_dir, stage_id, reason).unwrap_or_else(|error| {
        Response::Error {
            message: format!("Block transition failed: {error:#}"),
        }
    })
}

/// Serve one `FreezeContracts`. Every refusal comes back from the handler as
/// `Response::Error`; an `Err` means the freeze itself could not be written.
fn serve_freeze_contracts(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    reports: &[ContractRunReport],
) -> Response {
    super::contracts::handle_freeze_contracts(work_dir, stage_id, session_id, reports)
        .unwrap_or_else(|error| Response::Error {
            message: format!("Contract freeze failed: {error:#}"),
        })
}

/// Serve one of the requests a stage agent makes about its own stage, once
/// authorization and ownership are settled.
pub(super) fn serve_stage_control(work_dir: &Path, request: Request) -> Response {
    match request {
        Request::DisputeCriteria {
            stage_id,
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
            ..
        } => serve_dispute_criteria(
            work_dir,
            &stage_id,
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
        ),
        Request::FileDispute {
            stage_id,
            kind,
            reason,
            evidence_commit,
            ..
        } => serve_file_dispute(work_dir, &stage_id, kind, reason, evidence_commit),
        Request::BlockStage {
            stage_id, reason, ..
        } => serve_block_stage(work_dir, &stage_id, &reason),
        Request::FreezeContracts {
            stage_id,
            session_id,
            reports,
            ..
        } => serve_freeze_contracts(work_dir, &stage_id, &session_id, &reports),
        other => Response::Error {
            message: format!("{other:?} is not a stage-control request"),
        },
    }
}
