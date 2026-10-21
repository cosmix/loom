//! What an orchestrator run reports when it stops.

use chrono::{DateTime, Utc};

/// Result of orchestrator run
#[derive(Debug)]
pub struct OrchestratorResult {
    pub completed_stages: Vec<String>,
    /// Stages that ended in a terminal failure status (`Blocked`,
    /// `MergeConflict`, `CompletedWithFailures`, `MergeBlocked`,
    /// `NeedsHumanReview`) — the run cannot make progress on these without
    /// intervention.
    pub failed_stages: Vec<String>,
    /// Stages that were merely mid-flight when the run stopped (`Queued`,
    /// `WaitingForDeps`, `Executing`, `WaitingForInput`,
    /// `NeedsAdjudication`) — e.g. after `loom stop`. Distinct from
    /// `failed_stages`: nothing about these needs diagnosis, they just
    /// haven't finished.
    pub unfinished_stages: Vec<String>,
    pub needs_handoff: Vec<String>,
    pub total_sessions_spawned: usize,
    /// When the orchestrator started running
    pub started_at: DateTime<Utc>,
    /// When the orchestrator finished running
    pub completed_at: DateTime<Utc>,
}

impl OrchestratorResult {
    pub fn is_success(&self) -> bool {
        self.failed_stages.is_empty()
            && self.unfinished_stages.is_empty()
            && self.needs_handoff.is_empty()
    }
}
