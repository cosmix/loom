//! Live-session adoption at stage spawn time.
//!
//! Split out of `session_lifecycle.rs` to keep that file under the
//! maintainability limit. Adoption is now typed and escalates to `Blocked`
//! on a tracked-incumbent conflict instead of silently picking a session.

use anyhow::Result;
use chrono::Utc;

use crate::models::failure::FailureType;
use crate::models::stage::StageStatus;

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    /// Refuse to spawn a second agent over one that is still alive. A daemon
    /// crash can leave a stage `Executing` with a session that is
    /// unreachable (e.g. an orphaned tmux server) but still running; if the
    /// stage is later requeued (`loom stage reset`, or any other path that
    /// walks it back to `Queued`), scheduling it again here would spawn a
    /// duplicate agent into the same worktree alongside the first. Adopt the
    /// live session instead of spawning a duplicate.
    ///
    /// Only considers sessions of the stage's own WORKER kind (`Stage` for a
    /// standard stage, `Knowledge` for a knowledge stage): an adjudication
    /// session carries the stage's own `stage_id` and is not the agent doing
    /// the work, so it must never be adopted into the worker slot.
    ///
    /// Returns `Ok(true)` if a live session was found (and the spawn attempt
    /// should stop here, whether or not the adoption itself fully
    /// succeeded), `Ok(false)` if there is no live session to adopt.
    pub(super) fn adopt_live_session_if_present(&mut self, stage_id: &str) -> Result<bool> {
        let stage = self.load_stage(stage_id)?;
        let live_sessions = crate::orchestrator::session_registry::live_sessions_for_stage_of_type(
            &self.config.work_dir,
            stage_id,
            crate::orchestrator::coherence::worker_session_type(&stage),
        )?;
        let Some(newest) = live_sessions.into_iter().max_by_key(|s| s.created_at) else {
            return Ok(false);
        };

        // Check the incumbent BEFORE mutating anything: a different session
        // already tracked in memory for this stage means two agents may be
        // live for the same worktree, which is for an operator to resolve,
        // not for adoption to silently pick a side.
        let already_tracked = match self.active_sessions.get(stage_id) {
            Some(existing) if existing.id != newest.id => {
                self.escalate_adoption_conflict(stage_id, &newest.id, &existing.id.clone());
                return Ok(true);
            }
            Some(_) => true,
            None => false,
        };

        let session_id = newest.id.clone();
        if !self.link_adopted_session_to_stage(stage_id, &session_id) {
            return Ok(true);
        }
        if !already_tracked && !self.insert_active_session(stage_id, newest) {
            tracing::error!(
                stage_id = %stage_id,
                session_id = %session_id,
                "Adopted session could not be tracked; an active session already exists for this stage"
            );
        }
        Ok(true)
    }

    /// Escalate instead of adopting: a different session is already tracked
    /// in memory than the newest live session found on disk for this stage.
    /// Adopting either over the other would silently abandon whichever
    /// loses, so this marks the stage `Blocked` and names both sessions so
    /// an operator can decide.
    fn escalate_adoption_conflict(&mut self, stage_id: &str, newest_id: &str, existing_id: &str) {
        let reason = format!(
            "adoption refused: session '{newest_id}' is live for the stage but the daemon \
             already tracks '{existing_id}' for it; two agents may be working the same \
             worktree — take one down with 'loom stage reset {stage_id} --kill-session'"
        );
        match self.persist_blocked_stage(
            stage_id,
            FailureType::InfrastructureError,
            vec![reason.clone()],
        ) {
            Ok(()) => {
                let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
            }
            Err(error) => {
                tracing::error!(
                    stage_id = %stage_id,
                    %error,
                    "Failed to persist Blocked state while escalating an adoption conflict"
                );
            }
        }
        super::clear_status_line();
        eprintln!("{reason}");
    }

    /// Assign `session_id` to the stage and, if it is not already
    /// `Executing`, walk it there. Returns `false` on failure (already
    /// logged), in which case the caller must not proceed to tracking.
    fn link_adopted_session_to_stage(&mut self, stage_id: &str, session_id: &str) -> bool {
        tracing::warn!(
            stage_id = %stage_id,
            session_id = %session_id,
            "Adopting live session instead of spawning a duplicate agent"
        );
        if let Err(e) = self.update_stage(stage_id, |current| {
            current.assign_session(session_id.to_string());
            if current.status != StageStatus::Executing {
                current.try_mark_executing()?;
                current.begin_attempt(Utc::now());
            }
            Ok(())
        }) {
            tracing::error!(
                stage_id = %stage_id,
                session_id = %session_id,
                error = %e,
                "Failed to adopt live session"
            );
            return false;
        }
        if let Err(e) = self.graph.mark_executing(stage_id) {
            tracing::warn!(
                stage_id = %stage_id,
                error = %e,
                "Graph state out of sync while adopting a live session"
            );
        }
        true
    }
}
