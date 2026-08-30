//! The idempotent state transition shared by first and retry handoffs.

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::fs::session_files::{load_session_exact, record_session_context_exact};
use crate::handoff::HandoffOrigin;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::handlers::Handlers;

use super::super::Orchestrator;

pub(super) fn mark_needs_handoff(stage: &mut Stage, now: DateTime<Utc>) -> Result<()> {
    // A failed, fail-closed takedown deliberately leaves the stage here so
    // the monitor can retry. It is already outside an execution attempt;
    // repeating the same current-session handoff must neither fail the state
    // machine nor charge more attempt time.
    if stage.status == StageStatus::NeedsHandoff {
        return Ok(());
    }
    stage.accumulate_attempt_time(now);
    stage.try_mark_needs_handoff()
}

/// Failed takedowns retry every poll, but the same verified outgoing pair gets
/// one budget-origin handoff. Red, manual, legacy, or malformed artifacts do
/// not stand in for the enforcement snapshot.
pub(super) fn ensure_budget_handoff(
    handlers: &Handlers,
    session: &Session,
    stage: &Stage,
) -> Result<()> {
    if let Some(path) =
        handlers.ensure_context_handoff(session, stage, HandoffOrigin::BudgetExceeded)?
    {
        eprintln!("Generated handoff at: {}", path.display());
    }
    Ok(())
}

impl Orchestrator {
    /// Write the outgoing agent's handoff before the takedown takes it away.
    ///
    /// The persisted record wins over `active_sessions` for identity and
    /// backend fields: heartbeat updates are durable but do not refresh the
    /// daemon's spawn-time clone. The event's context reading wins over both,
    /// because it may come from a heartbeat not yet reflected on disk. A
    /// restarted daemon has no map entry at all. The caller already established
    /// that the stage still names `session_id`; exact persisted identity
    /// prevents a stale ceiling event from creating a handoff for a successor.
    pub(super) fn retire_exceeded_session(
        &mut self,
        session_id: &str,
        stage: &Stage,
        context_tokens: u32,
    ) -> Result<()> {
        // The BudgetExceeded event may have been computed from a heartbeat
        // fresher than the session file. Persist just that exact field before
        // creating the handoff; a full stale-session save would overwrite
        // unrelated concurrent updates.
        record_session_context_exact(&self.config.work_dir, session_id, &stage.id, context_tokens)?;
        let persisted = load_session_exact(&self.config.work_dir, session_id)?;
        if let Some(session) = persisted.as_ref() {
            anyhow::ensure!(
                session.stage_id.as_deref() == Some(stage.id.as_str()),
                "persisted session '{}' belongs to stage {:?}, not '{}'",
                session_id,
                session.stage_id,
                stage.id
            );
        };
        let session = persisted.or_else(|| {
            self.active_sessions
                .get(&stage.id)
                .filter(|session| session.id == session_id)
                .cloned()
        });
        let Some(mut session) = session else {
            return Ok(());
        };
        session.context_tokens = context_tokens;

        ensure_budget_handoff(self.monitor.handlers(), &session, stage)?;

        // `take_down_stage_agents` persists `ContextExhausted` only after it
        // has re-probed and confirmed the process gone. Marking this record
        // first would hide a restarted daemon's persisted agent from that
        // scan, letting a live process survive while its stage is re-queued.
        Ok(())
    }
}
