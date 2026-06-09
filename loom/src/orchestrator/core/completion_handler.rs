//! Stage completion handling
//!
//! The orchestrator kills the session and runs auto-merge against the
//! host repo directly.

use anyhow::Result;
use chrono::Utc;

use crate::orchestrator::signals::remove_signal;

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    pub(super) fn handle_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        // Accumulate execution time for the final attempt. A-4: a corrupt
        // stage file must be logged (with its path), not silently skipped.
        match self.load_stage(stage_id) {
            Ok(mut stage) => {
                stage.accumulate_attempt_time(Utc::now());
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: failed to save execution time for stage '{stage_id}': {e}");
                }
            }
            Err(e) => {
                let path = crate::fs::stage_files::find_stage_file(
                    &self.config.work_dir.join("stages"),
                    stage_id,
                )
                .ok()
                .flatten();
                tracing::error!(
                    stage_id = %stage_id,
                    path = ?path,
                    error = %e,
                    "Failed to load stage while recording completion time; continuing (corrupt stage file?)"
                );
            }
        }

        // Clean up session first
        if let Some(session) = self.active_sessions.remove(stage_id) {
            remove_signal(&session.id, &self.config.work_dir)?;
            let _ = self.native.kill_session(&session);
        }

        self.active_worktrees.remove(stage_id);

        // Attempt auto-merge if enabled BEFORE marking as completed
        // This allows us to detect merge conflicts and transition to MergeConflict status
        // instead of Completed, preventing dependent stages from starting prematurely
        let merge_succeeded = self.try_auto_merge(stage_id);

        // Only mark as completed if merge succeeded (or was not needed)
        // If merge failed with conflicts, stage will be in MergeConflict status instead
        if merge_succeeded {
            // O-4: a graph sync failure for one stage must not abort the
            // daemon. The next sync_graph_with_stage_files tick reconciles
            // the graph from the (already-persisted) stage file.
            if let Err(e) = self.graph.mark_completed(stage_id) {
                tracing::warn!(
                    stage_id = %stage_id,
                    error = %e,
                    "Failed to mark stage completed in graph; next sync will reconcile"
                );
            }
        }

        Ok(())
    }
}
