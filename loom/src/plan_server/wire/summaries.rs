//! Wire shapes for summary rebuilds.

use serde::{Deserialize, Serialize};

use crate::planning::summary::cost::estimate::EstimateBasis;
use crate::planning::summary::cost::{CostLine, TokenVector};
use crate::quota::model::QuotaSnapshot;

/// Progress of the currently running rebuild session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunningRebuild {
    /// Background session identifier.
    pub session: String,
    /// Summary identifiers assigned to the session.
    pub ids: Vec<String>,
    /// Number of summaries completed.
    pub done: usize,
    /// Number of summaries assigned.
    pub total: usize,
    /// Session start time.
    pub started_at: String,
}

/// A summary whose rebuild failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailedSummary {
    /// Summary identifier.
    pub id: String,
    /// Failure reason.
    pub error: String,
}

/// Approval request for a large rebuild.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryQuestion {
    /// Question identifier.
    pub id: String,
    /// Number of summaries in the request.
    pub count: usize,
    /// Summary identifiers awaiting approval.
    pub ids: Vec<String>,
    /// Estimated fresh tokens for the rebuild.
    pub estimate_tokens: u64,
    /// Evidence used for the estimate.
    pub basis: EstimateBasis,
}

/// Cost attributed to one summary during an authoring turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnCostRow {
    /// Summary identifier.
    pub id: String,
    /// Token count, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
}

/// Summary costs attributed to an authoring turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryTurnCost {
    /// Authoring session identifier.
    pub session: String,
    /// Turn identifier.
    pub turn: String,
    /// Costs attributed to summaries in the turn.
    pub rows: Vec<TurnCostRow>,
}

/// Current rebuild progress, costs, and available actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryPanel {
    /// Currently running rebuild, if any.
    pub running: Option<RunningRebuild>,
    /// Summary identifiers waiting to rebuild.
    pub queue: Vec<String>,
    /// Summaries whose rebuilds failed.
    pub failed: Vec<FailedSummary>,
    /// Approval question currently awaiting an answer.
    pub question: Option<SummaryQuestion>,
    /// Most recently attributed cost.
    pub last: Option<CostLine>,
    /// Token totals for today.
    pub today: TokenVector,
    /// Token totals for the current week.
    pub week: TokenVector,
    /// Cached provider quota state.
    pub quota: QuotaSnapshot,
    /// Reason rebuilding is read-only, if applicable.
    pub read_only: Option<String>,
    /// Summary identifiers claimed from a terminal.
    pub terminal_claims: Vec<String>,
    /// Most recent session start failure, until the next start.
    pub start_error: Option<String>,
}
