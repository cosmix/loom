//! Stage completion handling

use anyhow::Result;

use crate::orchestrator::signals::remove_signal;

use super::Orchestrator;

impl Orchestrator {
    pub(super) fn handle_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        // Clean up session first
        if let Some(session) = self.active_sessions.remove(stage_id) {
            remove_signal(&session.id, &self.config.work_dir)?;
            let _ = self.backend.kill_session(&session);
        }

        self.active_worktrees.remove(stage_id);

        // Attempt auto-merge if enabled BEFORE marking as completed
        // This allows us to detect merge conflicts and transition to MergeConflict status
        // instead of Completed, preventing dependent stages from starting prematurely
        let merge_succeeded = self.try_auto_merge(stage_id);

        // Only mark as completed if merge succeeded (or was not needed)
        // If merge failed with conflicts, stage will be in MergeConflict status instead
        if merge_succeeded {
            self.graph.mark_completed(stage_id)?;
        }

        Ok(())
    }
}
