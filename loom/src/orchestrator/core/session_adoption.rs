//! Live-session adoption at stage spawn time, and the graph resync for a
//! stage the executor must not start.
//!
//! Adoption walks only a `Queued` stage to `Executing`, and escalates to
//! `Blocked` on a tracked-incumbent conflict rather than picking a session.

use anyhow::Result;
use chrono::Utc;

use crate::models::failure::FailureType;
use crate::models::stage::StageStatus;
use crate::orchestrator::coherence::live_worker_sessions;
use crate::orchestrator::scheduling_report::BlockReason;

use super::persistence::Persistence;
use super::Orchestrator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdoptionResult {
    Adopted,
    Absent,
    Unknown,
}

impl Orchestrator {
    /// Bring the graph node of a stage the executor was handed as `Queued`
    /// in line with a stage file that says otherwise.
    ///
    /// The file is the authority: a stage whose file has moved on (to a
    /// dispute, a review, a handoff, a failure) since the last sync was put
    /// there on purpose, and starting it would walk it back out of that
    /// status — adoption re-linking a live agent, or a spawn putting a second
    /// one in the worktree. Resyncing the node takes it out of
    /// `ready_stages`, so the executor stops being handed it.
    pub(super) fn sync_unstartable_node(&mut self, stage_id: &str, status: &StageStatus) {
        let synced = match status {
            StageStatus::Executing => self.graph.mark_executing(stage_id),
            StageStatus::Completed => self.graph.mark_completed(stage_id).map(|_| ()),
            // A synchronisation, not a transition: `mark_status` would warn
            // that most of these are unreachable from `Queued`.
            other => self.graph.force_status(stage_id, other.clone()),
        };
        if let Err(error) = synced {
            tracing::warn!(
                stage_id = %stage_id,
                disk_status = %status,
                %error,
                "Failed to sync the graph node of a stage whose file is not startable"
            );
        }
    }

    /// Refuse to spawn a second agent over one that is still alive. A daemon
    /// crash can leave a stage `Executing` with a session that is
    /// unreachable (e.g. an orphaned tmux server) but still running; if the
    /// stage is later requeued (`loom stage reset`, or any other path that
    /// walks it back to `Queued`), scheduling it again here would spawn a
    /// duplicate agent into the same worktree alongside the first. Adopt the
    /// live session instead of spawning a duplicate.
    ///
    /// Adoption walks only a `Queued` stage to `Executing`; the executor calls
    /// this only for one (see `start_stage`), and the walk refuses any other
    /// status under the stage lock.
    ///
    /// Only considers sessions of the stage's own WORKER kind (`Stage` for a
    /// standard stage, `Knowledge` for a knowledge stage, falling back to
    /// `Contract` on a v2 standard stage in its contract phase): an
    /// adjudication session carries the stage's own `stage_id` and is not the
    /// agent doing the work, so it must never be adopted into the worker slot.
    ///
    /// Unknown inventory defers this spawn attempt without stopping the daemon.
    pub(super) fn adopt_live_session_if_present(
        &mut self,
        stage_id: &str,
    ) -> Result<AdoptionResult> {
        let stage = self.load_stage(stage_id)?;
        let live_sessions = match live_worker_sessions(&self.config.work_dir, &stage) {
            Ok(sessions) => sessions,
            Err(error) => {
                tracing::warn!(stage_id = %stage_id, %error,
                    "Session ownership is uncertain; deferring stage spawn");
                let detail = format!("session ownership is uncertain: {error:#}");
                self.spawn_blocks.insert(
                    stage_id.to_string(),
                    BlockReason::OwnershipUncertain { detail },
                );
                return Ok(AdoptionResult::Unknown);
            }
        };
        let Some(newest) = live_sessions.into_iter().max_by_key(|s| s.created_at) else {
            return Ok(AdoptionResult::Absent);
        };

        // Check the incumbent BEFORE mutating anything: a different session
        // already tracked in memory for this stage means two agents may be
        // live for the same worktree, which is for an operator to resolve,
        // not for adoption to silently pick a side.
        let already_tracked = match self.active_sessions.get(stage_id) {
            Some(existing) if existing.id != newest.id => {
                self.escalate_adoption_conflict(stage_id, &newest.id, &existing.id.clone());
                return Ok(AdoptionResult::Adopted);
            }
            Some(_) => true,
            None => false,
        };

        let session_id = newest.id.clone();
        if !self.link_adopted_session_to_stage(stage_id, &session_id) {
            return Ok(AdoptionResult::Adopted);
        }
        if !already_tracked && !self.insert_active_session(stage_id, newest) {
            tracing::error!(
                stage_id = %stage_id,
                session_id = %session_id,
                "Adopted session could not be tracked; an active session already exists for this stage"
            );
        }
        Ok(AdoptionResult::Adopted)
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

    /// Assign `session_id` to the `Queued` stage and walk it to `Executing`.
    /// Returns `false` on failure (already logged), including a stage whose
    /// file is no longer `Queued`, in which case the caller must not proceed
    /// to tracking.
    fn link_adopted_session_to_stage(&mut self, stage_id: &str, session_id: &str) -> bool {
        tracing::warn!(
            stage_id = %stage_id,
            session_id = %session_id,
            "Adopting live session instead of spawning a duplicate agent"
        );
        if let Err(e) = self.update_stage(stage_id, |current| {
            anyhow::ensure!(
                current.status == StageStatus::Queued,
                "stage is {}, not Queued; a live session is adopted only into a queued stage",
                current.status
            );
            current.assign_session(session_id.to_string());
            current.try_mark_executing()?;
            current.begin_attempt(Utc::now());
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

#[cfg(test)]
#[path = "session_adoption_tests.rs"]
mod session_adoption_tests;
