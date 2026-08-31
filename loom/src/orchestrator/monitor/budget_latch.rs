//! Backstop-latch lifetime and the handoff retry it authorizes.

use std::collections::HashSet;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};

use super::ceiling::session_has_current_assignment;
use super::detection::Detection;
use super::events::MonitorEvent;

impl Detection {
    /// Forget backstop state that cannot authorize a retry in this snapshot.
    /// Session records accumulate across attempts; retaining a predecessor's
    /// latch would suppress its successor's normal NeedsHandoff recovery.
    pub(super) fn clear_inactive_latches(&mut self, sessions: &[Session]) {
        let running_session_ids: HashSet<_> = sessions
            .iter()
            .filter(|session| session.status == SessionStatus::Running)
            .map(|session| session.id.as_str())
            .collect();
        self.last_budget_exceeded
            .retain(|session_id, _| running_session_ids.contains(session_id.as_str()));
        self.red_handoff_ready
            .retain(|session_id| running_session_ids.contains(session_id.as_str()));
    }

    /// Whether this session may be judged against a ceiling on this tick, and
    /// the latch cleanup owed to it when it may not.
    ///
    /// Only a session that is actually RUNNING may be judged. `terminal` covers
    /// a session that went terminal on THIS tick; a record already persisted as
    /// Crashed/Completed/ContextExhausted still carries its last high
    /// `context_tokens`, and judging that corpse fires the backstop against
    /// whatever agent the stage is running NOW. Same guard, for the same
    /// reason, as `detect_heartbeat_events`.
    pub(super) fn judgeable(
        &mut self,
        session: &Session,
        stages: &[Stage],
        terminal: bool,
    ) -> bool {
        if terminal || session.status != SessionStatus::Running {
            self.last_budget_exceeded.remove(&session.id);
            self.red_handoff_ready.remove(&session.id);
            return false;
        }
        if !session_has_current_assignment(session, stages) {
            self.last_context_levels.remove(&session.id);
            self.last_budget_exceeded.remove(&session.id);
            self.red_handoff_ready.remove(&session.id);
            return false;
        }
        true
    }

    /// Keep generic handoff retries level-triggered unless a matching live
    /// budget latch owns the retry and will re-emit `BudgetExceeded` instead.
    pub(super) fn needs_handoff_event(&self, stage: &Stage) -> Option<MonitorEvent> {
        (!self.budget_retry_owns(stage))
            .then(|| needs_handoff_event(stage))
            .flatten()
    }

    fn budget_retry_owns(&self, stage: &Stage) -> bool {
        stage.status == StageStatus::NeedsHandoff
            && stage
                .session
                .as_ref()
                .is_some_and(|session_id| self.last_budget_exceeded.get(session_id) == Some(&true))
    }
}

/// Keep retrying fail-closed handoff work after transient uncertainty clears.
fn needs_handoff_event(stage: &Stage) -> Option<MonitorEvent> {
    (stage.status == StageStatus::NeedsHandoff)
        .then_some(stage.session.as_ref())
        .flatten()
        .map(|session_id| MonitorEvent::SessionNeedsHandoff {
            session_id: session_id.clone(),
            stage_id: stage.id.clone(),
        })
}
