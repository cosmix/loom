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
        let completed_at = Utc::now();
        if let Err(e) = self.update_stage(stage_id, |stage| {
            stage.accumulate_attempt_time(completed_at);
            Ok(())
        }) {
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
                "Failed to update stage while recording completion time; continuing (corrupt stage file?)"
            );
        }

        // Clean up session first.
        //
        // Every step here is best-effort and must stay that way. Teardown
        // touches the outside world (signal files, the terminal's window
        // manager, the agent process); the merge and the graph update below
        // are what let DEPENDENT stages run. Propagating a teardown error
        // aborts this handler before `try_auto_merge`, and because
        // `StageCompleted` is edge-triggered off a status change
        // (monitor/detection.rs), it never fires again for this stage — the
        // dependents then sit Queued until the daemon is restarted. Leaving a
        // stale signal file or an unkilled agent is strictly cheaper than
        // stalling the plan, so log and continue.
        if let Some(session) = self.active_sessions.remove(stage_id) {
            if let Err(e) = remove_signal(&session.id, &self.config.work_dir) {
                tracing::warn!(
                    stage_id = %stage_id,
                    session_id = %session.id,
                    error = %e,
                    "Failed to remove signal during completion; continuing with merge"
                );
            }
            if let Err(e) = self.backend.kill_session(&session) {
                tracing::warn!(
                    stage_id = %stage_id,
                    session_id = %session.id,
                    error = %e,
                    "Failed to kill session during completion; the agent process may \
                     still be running. Continuing with merge."
                );
            }
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
