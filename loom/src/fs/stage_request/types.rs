//! What a stage agent can ask the daemon to do on its behalf.

use serde::{Deserialize, Serialize};

use crate::daemon::ContractRunReport;
use crate::models::dispute::DisputeKind;

/// One queued stage-control request, mirroring the `Request::BlockStage`,
/// `Request::DisputeCriteria`, `Request::FileDispute` and
/// `Request::FreezeContracts` RPCs field for field.
///
/// The RPC variants carry `stage_id` and `session_id`; these deliberately do
/// not. Over the socket those fields are checked against the connection's peer
/// identity, so claiming a stage you do not own gets you refused. A spool has
/// no connection to check against, so the fields are simply absent and the
/// daemon attributes the request to the worktree it found it in — see the
/// module documentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum StageRequest {
    /// `loom stage block <id> "<reason>"`.
    Block { reason: String },
    /// `loom stage dispute-criteria <id> --criterion-index N --reason "..."`.
    Dispute {
        criterion_index: usize,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence_commit: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failure_output: Option<String>,
    },
    /// `loom stage contracts freeze <id>`.
    FreezeContracts { reports: Vec<ContractRunReport> },
    /// `loom stage dispute-findings|dispute-contract|dispute-integrity <id> ...`.
    FileDispute {
        kind: DisputeKind,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence_commit: Option<String>,
    },
}

impl StageRequest {
    /// Short label for logs: enough to tell an operator which command a
    /// drained line came from, without reproducing an agent-written reason
    /// into every log record.
    pub fn kind(&self) -> &'static str {
        match self {
            StageRequest::Block { .. } => "block",
            StageRequest::Dispute { .. } => "dispute",
            StageRequest::FreezeContracts { .. } => "freeze_contracts",
            StageRequest::FileDispute { .. } => "file_dispute",
        }
    }
}
