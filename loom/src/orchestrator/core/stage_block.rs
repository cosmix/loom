//! The orchestrator's single "block this stage" transition: persist `Blocked`
//! under the stage lock, then mark the in-memory execution graph to match.

use chrono::Utc;

use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::StageStatus;

use super::persistence::Persistence;
use super::Orchestrator;

/// What a block records on the stage besides the `Blocked` status.
pub(super) struct StageBlock {
    pub failure_type: FailureType,
    pub evidence: Vec<String>,
    /// Written to `stage.close_reason` when `Some`; left as it was when `None`.
    pub close_reason: Option<String>,
    /// Clear `stage.session` in the same locked update.
    pub release_session: bool,
}

impl StageBlock {
    /// A block with no close reason that keeps the stage's session link.
    pub(super) fn new(failure_type: FailureType, evidence: Vec<String>) -> Self {
        Self {
            failure_type,
            evidence,
            close_reason: None,
            release_session: false,
        }
    }
}

impl Orchestrator {
    /// Persist `Blocked` with `block`, then mark the graph `Blocked`. Returns
    /// whether the stage is now Blocked on disk. A persistence failure
    /// (including a refused transition) is logged with `tracing::error`
    /// naming the stage and leaves the graph untouched; a graph failure after
    /// a successful persist is logged with `tracing::warn` and still returns
    /// true.
    pub(super) fn block_stage(&mut self, stage_id: &str, block: StageBlock) -> bool {
        let StageBlock {
            failure_type,
            evidence,
            close_reason,
            release_session,
        } = block;
        let persisted = self.update_stage(stage_id, |current| {
            current.try_mark_blocked()?;
            current.failure_info = Some(FailureInfo {
                failure_type,
                detected_at: Utc::now(),
                evidence,
            });
            if let Some(reason) = close_reason {
                current.close_reason = Some(reason);
            }
            if release_session {
                current.release_session();
            }
            Ok(())
        });
        if let Err(error) = persisted {
            tracing::error!(stage_id, "failed to persist Blocked state: {error:#}");
            return false;
        }
        if let Err(error) = self.graph.mark_status(stage_id, StageStatus::Blocked) {
            tracing::warn!(
                stage_id,
                "stage persisted Blocked but graph not marked: {error:#}"
            );
        }
        true
    }
}

#[cfg(test)]
#[path = "stage_block_tests.rs"]
mod tests;
