//! Serving the requests a stage agent makes about its own stage — its
//! disputes, its block, its contract freeze and the fingerprint of its
//! changes — once `client.rs` has settled authorization and stage ownership.
//! Each handler's failure comes back as a `Response::Error` naming it.

use std::path::Path;

use crate::daemon::protocol::{Request, Response};
use crate::models::dispute::{CriterionField, DisputeKind};

/// Serve one `DisputeCriteria`.
///
/// The handler owns `request.md` persistence and the transition to
/// `NeedsAdjudication`. Authorization and stage ownership are settled before
/// it runs; the handler additionally validates `criterion_indices` and the
/// stage's dispute budget.
fn serve_dispute_criteria(
    work_dir: &Path,
    stage_id: &str,
    field: CriterionField,
    criterion_indices: Vec<usize>,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
) -> Response {
    super::dispute::handle_dispute_criteria(
        work_dir,
        stage_id,
        field,
        criterion_indices,
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
/// from the handler as `Response::Error`, not as an `Err`. A request naming a
/// session comes from a stage agent and is refused while the latest completion
/// shows a failing criterion; the operator's, with an empty session id, blocks
/// unconditionally.
fn serve_block_stage(work_dir: &Path, stage_id: &str, session_id: &str, reason: &str) -> Response {
    let blocked = if session_id.is_empty() {
        super::control_block::handle_block_stage(work_dir, stage_id, reason)
    } else {
        super::control_block::handle_agent_block_stage(work_dir, stage_id, reason)
    };
    blocked.unwrap_or_else(|error| Response::Error {
        message: format!("Block transition failed: {error:#}"),
    })
}

/// Serve one `FreezeContracts`: queue the host contract run. Every refusal
/// comes back from the handler as `Response::Error`; an `Err` means the job
/// could not be queued. A socket request carries no relay id, so the job is
/// joined under a fresh one.
fn serve_freeze_contracts(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    head: &str,
    tree_digest: &str,
) -> Response {
    let request_id = crate::host_verify::new_job_id();
    super::contracts::handle_freeze_contracts(
        work_dir,
        stage_id,
        session_id,
        head,
        tree_digest,
        &request_id,
    )
    .unwrap_or_else(|error| Response::Error {
        message: format!("Contract run request failed: {error:#}"),
    })
}

/// Serve one of the requests a stage agent makes about its own stage, once
/// authorization and ownership are settled.
pub(super) fn serve_stage_control(work_dir: &Path, request: Request) -> Response {
    match request {
        Request::DisputeCriteria {
            stage_id,
            field,
            criterion_indices,
            reason,
            evidence_commit,
            failure_output,
            ..
        } => serve_dispute_criteria(
            work_dir,
            &stage_id,
            field,
            criterion_indices,
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
            stage_id,
            session_id,
            reason,
            ..
        } => serve_block_stage(work_dir, &stage_id, &session_id, &reason),
        Request::FreezeContracts {
            stage_id,
            session_id,
            head,
            tree_digest,
            ..
        } => serve_freeze_contracts(work_dir, &stage_id, &session_id, &head, &tree_digest),
        Request::ObserveChanges { stage_id, .. } => {
            super::observer::handle_observe_changes(work_dir, &stage_id)
        }
        other => Response::Error {
            message: format!("{other:?} is not a stage-control request"),
        },
    }
}
