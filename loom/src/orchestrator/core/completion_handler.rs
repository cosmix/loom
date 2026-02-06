//! Stage completion handling

use anyhow::Result;
use chrono::Utc;

use crate::orchestrator::signals::remove_signal;

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    pub(super) fn handle_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        // Accumulate execution time for the final attempt
        if let Ok(mut stage) = self.load_stage(stage_id) {
            stage.accumulate_attempt_time(Utc::now());
            if let Err(e) = self.save_stage(&stage) {
                eprintln!("Warning: failed to save execution time for stage '{stage_id}': {e}");
            }
        }

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
