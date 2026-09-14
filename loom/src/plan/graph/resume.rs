use anyhow::{bail, Result};

use crate::models::stage::StageStatus;

use super::ExecutionGraph;

impl ExecutionGraph {
    /// Restore execution after a stage has received the input it was awaiting.
    pub fn mark_resumed(&mut self, stage_id: &str) -> Result<()> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;

        match node.status.clone() {
            StageStatus::WaitingForInput => node.status = StageStatus::Executing,
            StageStatus::Executing => {}
            status => bail!(
                "Stage '{}' cannot resume from status {:?}",
                stage_id,
                status
            ),
        }
        Ok(())
    }
}
