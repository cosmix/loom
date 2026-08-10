//! What a session's observed status means for its stage.
//!
//! Split out of `detection`, which keeps stage, context-health and heartbeat
//! detection. This half is subtle enough to read on its own: it decides when a
//! vanished process is a crash rather than a normal exit, and — the part that
//! has bitten hardest — when an observation is a real transition rather than
//! the daemon simply seeing a session file for the first time after a restart.

use std::path::Path;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};

use super::detection::Detection;
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::remove_heartbeat;

/// Remove the on-disk heartbeat file for a session's stage when the session
/// reaches a terminal status (crash/completion). Heartbeat files are keyed by
/// stage ID, so leaving a dead session's heartbeat behind lets it later flag a
/// fresh session that reuses the same stage as hung. Best-effort: a failure to
/// remove is logged but never blocks detection.
pub(super) fn cleanup_heartbeat_for_session(work_dir: &Path, session: &Session) {
    if let Some(stage_id) = &session.stage_id {
        if let Err(e) = remove_heartbeat(work_dir, stage_id) {
            tracing::warn!(
                "Failed to remove heartbeat for stage '{}' (session '{}'): {}",
                stage_id,
                session.id,
                e
            );
        }
    }
}

/// Whether `session` is the session its stage currently points at.
///
/// Session files accumulate — a stage that crashed and retried leaves every
/// previous session on disk with `stage_id` still set — so "this session names
/// stage X" is far weaker than "stage X is being executed by this session".
/// Only the latter licenses a session to speak for its stage.
fn is_stage_active_session(session: &Session, stages: &[Stage]) -> bool {
    let Some(stage_id) = &session.stage_id else {
        return false;
    };
    stages
        .iter()
        .any(|stage| &stage.id == stage_id && stage.session.as_deref() == Some(session.id.as_str()))
}

/// One session's status observation.
pub(super) struct SessionStatusEvents {
    pub events: Vec<MonitorEvent>,
    /// The session reached a terminal state on this tick, so its context and
    /// budget checks are skipped for the rest of the poll.
    pub terminal: bool,
}

impl SessionStatusEvents {
    fn ongoing(events: Vec<MonitorEvent>) -> Self {
        Self {
            events,
            terminal: false,
        }
    }

    fn terminal(events: Vec<MonitorEvent>) -> Self {
        Self {
            events,
            terminal: true,
        }
    }
}

/// Whether this observation is the daemon seeing a session file for the FIRST
/// time rather than watching it change — and, if so, whether that session has
/// any standing to speak for its stage.
///
/// The distinction only matters on daemon startup, when `last_session_states`
/// is empty and every session file on disk is a first observation, including
/// corpses from hours ago that were already handled and persisted as Crashed.
/// Replaying those is not cosmetic: a crash report is written before any
/// handler guard runs, and the emitted event is charged to the stage's retry
/// budget. Observed 2026-08-10 — a restart re-fired a 25-minute-old crash,
/// blocked a healthy stage, auto-retried it, and put a second agent into a
/// worktree another agent was still working in.
///
/// A first observation that IS the stage's active session still speaks: that is
/// a stage genuinely stranded by a daemon that died between the crash and
/// handling it, and it must recover.
fn replays_stale_session(
    previous_status: Option<&SessionStatus>,
    session: &Session,
    stages: &[Stage],
) -> bool {
    previous_status.is_none() && !is_stage_active_session(session, stages)
}

/// Events owed for a real status transition.
fn transition_events(
    session: &Session,
    current_status: &SessionStatus,
    handlers: &Handlers,
) -> Vec<MonitorEvent> {
    match current_status {
        // Check if this is a merge session that completed
        SessionStatus::Completed if handlers.is_merge_session(&session.id) => session
            .stage_id
            .iter()
            .map(|stage_id| MonitorEvent::MergeSessionCompleted {
                session_id: session.id.clone(),
                stage_id: stage_id.clone(),
            })
            .collect(),
        SessionStatus::Crashed => {
            // Generate crash report
            let crash_report_path =
                handlers.handle_session_crash(session, "Session marked as crashed");
            vec![MonitorEvent::SessionCrashed {
                session_id: session.id.clone(),
                stage_id: session.stage_id.clone(),
                crash_report_path,
            }]
        }
        _ => Vec::new(),
    }
}

impl Detection {
    /// Detect what one session's status means, updating the last-seen state.
    pub(super) fn detect_session_status(
        &mut self,
        session: &Session,
        stages: &[Stage],
        handlers: &Handlers,
    ) -> SessionStatusEvents {
        let previous_status = self.last_session_states.get(&session.id).cloned();
        let current_status = &session.status;

        if previous_status == Some(SessionStatus::Running)
            && current_status == &SessionStatus::Running
        {
            if let Some(outcome) = self.detect_vanished_process(session, stages, handlers) {
                return outcome;
            }
        }

        if previous_status.as_ref() == Some(current_status) {
            return SessionStatusEvents::ongoing(Vec::new());
        }

        let events = if replays_stale_session(previous_status.as_ref(), session, stages) {
            tracing::debug!(
                session_id = %session.id,
                stage_id = ?session.stage_id,
                status = ?current_status,
                "Seeding first observation of a stale session without emitting"
            );
            Vec::new()
        } else {
            transition_events(session, current_status, handlers)
        };

        self.last_session_states
            .insert(session.id.clone(), current_status.clone());
        SessionStatusEvents::ongoing(events)
    }

    /// A session recorded as Running whose process is gone. Returns `None` when
    /// the process is alive or has no trackable identity, in which case the
    /// caller falls through to ordinary transition detection.
    fn detect_vanished_process(
        &mut self,
        session: &Session,
        stages: &[Stage],
        handlers: &Handlers,
    ) -> Option<SessionStatusEvents> {
        // If check_session_alive returns Ok(None), the session has no trackable
        // process, so we skip liveness checking
        if !matches!(handlers.check_session_alive(session), Ok(Some(false))) {
            return None;
        }
        self.finished_merge_session(session, handlers)
            .or_else(|| self.exited_after_stage_finished(session, stages, handlers))
            .or_else(|| Some(self.record_crash(session, handlers)))
    }

    /// A merge session whose process exited has completed its resolution.
    fn finished_merge_session(
        &mut self,
        session: &Session,
        handlers: &Handlers,
    ) -> Option<SessionStatusEvents> {
        if !handlers.is_merge_session(&session.id) {
            return None;
        }
        let stage_id = session.stage_id.as_ref()?;
        let events = vec![MonitorEvent::MergeSessionCompleted {
            session_id: session.id.clone(),
            stage_id: stage_id.clone(),
        }];
        self.mark_finished(session, handlers);
        Some(SessionStatusEvents::terminal(events))
    }

    /// The session exited normally after its stage already reached a terminal
    /// state. Without this the ordinary exit would be filed as a crash.
    fn exited_after_stage_finished(
        &mut self,
        session: &Session,
        stages: &[Stage],
        handlers: &Handlers,
    ) -> Option<SessionStatusEvents> {
        let stage_id = session.stage_id.as_ref()?;
        let stage = stages.iter().find(|s| &s.id == stage_id)?;
        if !matches!(
            stage.status,
            StageStatus::Completed | StageStatus::MergeConflict | StageStatus::MergeBlocked
        ) {
            return None;
        }
        self.mark_finished(session, handlers);
        Some(SessionStatusEvents::terminal(Vec::new()))
    }

    /// Persist a normal completion and drop the session's heartbeat.
    fn mark_finished(&mut self, session: &Session, handlers: &Handlers) {
        handlers.persist_session_status(session, SessionStatus::Completed);
        cleanup_heartbeat_for_session(handlers.work_dir(), session);
        self.last_session_states
            .insert(session.id.clone(), SessionStatus::Completed);
    }

    /// The process is gone with no benign explanation: file a crash.
    fn record_crash(&mut self, session: &Session, handlers: &Handlers) -> SessionStatusEvents {
        let reason = if session.pid.is_some() {
            "Process no longer running"
        } else {
            "Session no longer running"
        };
        let crash_report_path = handlers.handle_session_crash(session, reason);
        handlers.persist_session_status(session, SessionStatus::Crashed);
        // Remove the now-dead session's heartbeat so it can't later flag a
        // fresh session reusing this stage as hung.
        cleanup_heartbeat_for_session(handlers.work_dir(), session);
        self.last_session_states
            .insert(session.id.clone(), SessionStatus::Crashed);
        SessionStatusEvents::terminal(vec![MonitorEvent::SessionCrashed {
            session_id: session.id.clone(),
            stage_id: session.stage_id.clone(),
            crash_report_path,
        }])
    }
}

#[cfg(test)]
mod tests;
