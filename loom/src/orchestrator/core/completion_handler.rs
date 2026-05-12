//! Stage completion handling
//!
//! For native sessions, the orchestrator kills the session and runs
//! auto-merge against the host repo directly.
//!
//! For container sessions, the agent's in-container `loom stage
//! complete` no longer mutates host-side stage state. Instead it
//! sends [`Request::CompleteStageContainer`] over the daemon's Unix
//! socket. The daemon-side handler (which lives in
//! `daemon::server::client::complete_stage_container`) uses
//! [`git_bridge`] to extract a git bundle from the LIVE container,
//! validates and imports it before auto_merge runs here. Only after
//! that import succeeds does the orchestrator kill the session and
//! write the final stage state — this ordering is the architectural
//! fix for Codex blocker B6 (commits lost when the session is killed
//! too early).

use anyhow::Result;
use chrono::Utc;

// Re-export site so the completion handler is the documented entry
// point for the host-authoritative container extraction flow. The
// orchestrator uses [`git_bridge::cleanup_mirror`] after a
// container-mode stage's commits have been imported into the host
// repo, and the wiring check `pattern git_bridge in
// completion_handler.rs` relies on this reference being present.
use crate::orchestrator::signals::remove_signal;
#[allow(unused_imports)]
use crate::orchestrator::terminal::container::git_bridge;

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

        // Snapshot the session backend before removal — used after
        // auto-merge to decide whether to clean up the per-stage
        // bare mirror created by git_bridge::init_bare_mirror.
        let container_session = self
            .active_sessions
            .get(stage_id)
            .map(|s| s.backend == crate::plan::schema::execution::BackendType::Container)
            .unwrap_or(false);

        // Clean up session first
        if let Some(session) = self.active_sessions.remove(stage_id) {
            remove_signal(&session.id, &self.config.work_dir)?;
            let kill_result = self.dispatcher.for_session(&session).kill_session(&session);
            if kill_result.is_ok()
                && session.backend == crate::plan::schema::execution::BackendType::Container
            {
                let mut updated_session = session.clone();
                updated_session.clear_container_identity();
                if let Err(e) = self.save_session(&updated_session) {
                    eprintln!("Warning: failed to clear container identity after removal: {e}");
                }
            }
        }

        self.active_worktrees.remove(stage_id);

        // Attempt auto-merge if enabled BEFORE marking as completed
        // This allows us to detect merge conflicts and transition to MergeConflict status
        // instead of Completed, preventing dependent stages from starting prematurely
        let merge_succeeded = self.try_auto_merge(stage_id);

        // For container-mode stages: clean up the per-stage bare
        // mirror after a successful import + merge. Errors are
        // intentionally swallowed — leftover mirrors cost disk space
        // but never cause correctness issues (the next spawn replaces
        // them anyway).
        if container_session && merge_succeeded {
            if let Err(e) = git_bridge::cleanup_mirror(&self.config.work_dir, stage_id) {
                tracing::debug!(
                    stage_id = %stage_id,
                    error = %e,
                    "Best-effort bare-mirror cleanup failed; will be re-created on next spawn"
                );
            }
        }

        // Only mark as completed if merge succeeded (or was not needed)
        // If merge failed with conflicts, stage will be in MergeConflict status instead
        if merge_succeeded {
            self.graph.mark_completed(stage_id)?;
        }

        Ok(())
    }
}
