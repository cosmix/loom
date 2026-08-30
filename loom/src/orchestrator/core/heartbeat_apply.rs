//! Applying an observed heartbeat to the session record it describes.
//!
//! # Why this is a module and not four lines in `event_handler`
//!
//! `HeartbeatWatcher` polls `.work/heartbeat/<stage-id>.json` and the monitor
//! turns every update into a [`MonitorEvent::HeartbeatReceived`]. Until this
//! module existed that event's handler was an empty block whose comment claimed
//! the data was "just used for internal tracking" — nothing tracked it, so
//! `Session::last_active` kept its spawn timestamp for the whole life of every
//! session and `context_tokens` stayed `0`.
//!
//! Observed 2026-08-24 on an eight-stage plan: all five session records carried
//! `last_active` equal to their spawn timestamp and `context_tokens: 0`, while
//! `.work/heartbeat/<stage>.json` was being rewritten correctly by the hooks the
//! whole time. The data was arriving and being dropped one layer above where it
//! was needed. Every consumer of the figure — the status dashboard's context
//! column, the ceiling comparison, the resumed agent's signal — was reading a
//! constant zero.
//!
//! # What this does NOT change
//!
//! Hung detection stays advisory. `MonitorEvent::SessionHung` still only warns —
//! nothing here kills, retries, or transitions a stage, and that remains a
//! deliberate design decision documented at its handler in `event_handler`.
//! This module makes the session record true; it does not add a new policy.

use anyhow::Result;

use crate::fs::session_files::record_session_heartbeat_exact;

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    /// Record a heartbeat against the session it names.
    ///
    /// Best-effort by design: a heartbeat is an observation, not a command, so
    /// a missing or unparseable session file is logged and skipped rather than
    /// failing the tick. `handle_events` isolates handler errors anyway, but a
    /// heartbeat must not be the thing that produces them — one arrives after
    /// every tool call in every live session.
    pub(super) fn apply_heartbeat(
        &self,
        stage_id: &str,
        session_id: &str,
        context_tokens: Option<u32>,
        transcript_path: Option<String>,
    ) -> Result<()> {
        let applied = record_session_heartbeat_exact(
            self.persistence_work_dir(),
            session_id,
            stage_id,
            context_tokens,
            transcript_path,
        )?;
        if !applied {
            tracing::debug!(
                stage_id = %stage_id,
                session_id = %session_id,
                "Heartbeat does not name this stage's exact live session; ignoring"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::models::session::{Session, SessionStatus};
    use chrono::{Duration, Utc};

    fn running_session(stage_id: &str) -> Session {
        let mut session = Session::new();
        session.assign_to_stage(stage_id.to_string());
        session.status = SessionStatus::Running;
        session
    }

    /// The regression this whole module exists for: before it, `last_active`
    /// was written once at spawn and never again, so a session that had been
    /// working for hours still reported its spawn timestamp.
    ///
    /// The spawn time is backdated rather than read from a second `Utc::now()`
    /// — two adjacent `now()` calls can land in the same clock tick on macOS,
    /// which makes a strict `>` against a just-spawned session flaky without
    /// testing anything about the fix.
    #[test]
    fn heartbeat_advances_last_active_off_the_spawn_timestamp() {
        let mut session = running_session("build");
        let spawned_at = Utc::now() - Duration::hours(3);
        session.last_active = spawned_at;

        session.record_heartbeat(None, None);

        assert!(
            session.last_active > spawned_at,
            "a heartbeat must move last_active off the spawn timestamp"
        );
    }

    #[test]
    fn a_token_reading_replaces_the_previous_one() {
        let mut session = running_session("build");
        session.record_heartbeat(Some(91_000), None);
        assert_eq!(session.context_tokens, 91_000);
        session.record_heartbeat(Some(147_000), None);
        assert_eq!(session.context_tokens, 147_000);
    }

    /// A hook that could not measure the transcript reports `None`. That is
    /// ignorance, not a context of zero: zeroing the field would retract a
    /// handoff the ceiling comparison had already made due.
    #[test]
    fn a_missing_reading_preserves_the_previous_one() {
        let mut session = running_session("build");
        session.record_heartbeat(Some(147_000), None);
        session.record_heartbeat(None, None);
        assert_eq!(session.context_tokens, 147_000);
    }

    /// The transcript path is written once and never nulled: a later heartbeat
    /// without it means the hook did not resend the path, not that the
    /// transcript stopped existing.
    #[test]
    fn the_transcript_path_survives_a_heartbeat_that_omits_it() {
        let mut session = running_session("build");
        assert_eq!(session.transcript_path, None);

        session.record_heartbeat(None, Some("/t/a.jsonl".to_string()));
        assert_eq!(session.transcript_path, Some("/t/a.jsonl".to_string()));

        session.record_heartbeat(Some(1_000), None);
        assert_eq!(session.transcript_path, Some("/t/a.jsonl".to_string()));

        session.record_heartbeat(None, Some("/t/b.jsonl".to_string()));
        assert_eq!(session.transcript_path, Some("/t/b.jsonl".to_string()));
    }
}
