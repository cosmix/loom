use anyhow::Result;

use crate::models::stage::StageStatus;
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::Orchestrator;

impl Orchestrator {
    pub(crate) fn resumed_stage_is_executing(&self, stage_id: &str) -> Result<bool> {
        let stage = self.load_stage(stage_id)?;
        if stage.status == StageStatus::Executing {
            Ok(true)
        } else {
            tracing::debug!(
                stage_id,
                disk_status = ?stage.status,
                "Ignoring stale stage resume event"
            );
            Ok(false)
        }
    }

    pub(crate) fn sync_resumed_node(&mut self, stage_id: &str) {
        let status = self
            .graph
            .get_node(stage_id)
            .map(|node| node.status.clone());
        let result = match status {
            Some(StageStatus::WaitingForInput) => self.graph.mark_resumed(stage_id),
            Some(StageStatus::Queued) => self.graph.mark_executing(stage_id),
            Some(StageStatus::WaitingForDeps) => {
                self.graph.force_status(stage_id, StageStatus::Executing)
            }
            Some(StageStatus::Executing) | Some(_) | None => return,
        };
        if let Err(error) = result {
            tracing::warn!(stage_id, %error, "Failed to sync executing graph node");
        }
    }
}
