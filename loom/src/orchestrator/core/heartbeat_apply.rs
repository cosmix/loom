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

use crate::fs::locking::locked_read;
use crate::fs::session_files::{find_session_file, save_session};
use crate::models::session::{Session, SessionStatus};
use crate::parser::frontmatter::parse_from_markdown;

use super::persistence::Persistence;
use super::Orchestrator;

/// Whether an observed heartbeat may write to `session`.
///
/// Heartbeat files are keyed by STAGE, but session records accumulate: a stage
/// that crashed and retried leaves every previous session on disk with its
/// `stage_id` still set. So "this heartbeat names the stage this session names"
/// is far weaker than "this session is the one currently running that stage",
/// and only the strong form licenses a write. This is the same rule, for the
/// same reason, as `is_stage_active_session` in `monitor/session_events.rs` and
/// `stage_answerable_for_crash` in `core/crash_handler.rs`.
///
/// The session id is matched by the caller — it looks the record up by the id
/// the heartbeat carries — so what is left to check here is that the record is
/// still a live session for the stage the heartbeat is about.
fn heartbeat_applies_to(session: &Session, stage_id: &str) -> bool {
    if session.status != SessionStatus::Running {
        return false;
    }
    session.stage_id.as_deref() == Some(stage_id)
}

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
        let work_dir = self.persistence_work_dir();

        let Some(path) = find_session_file(work_dir, session_id)? else {
            tracing::debug!(
                stage_id = %stage_id,
                session_id = %session_id,
                "Heartbeat names a session with no file on disk; ignoring"
            );
            return Ok(());
        };

        let mut session: Session = parse_from_markdown(&locked_read(&path)?, "Session")?;

        if !heartbeat_applies_to(&session, stage_id) {
            tracing::debug!(
                stage_id = %stage_id,
                session_id = %session_id,
                status = ?session.status,
                "Heartbeat does not apply: session is not the stage's live session"
            );
            return Ok(());
        }

        session.record_heartbeat(context_tokens, transcript_path);
        save_session(&session, work_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn running_session(stage_id: &str) -> Session {
        let mut session = Session::new();
        session.assign_to_stage(stage_id.to_string());
        session.status = SessionStatus::Running;
        session
    }

    #[test]
    fn applies_to_the_stages_live_session() {
        let session = running_session("build");
        assert!(heartbeat_applies_to(&session, "build"));
    }

    #[test]
    fn refuses_a_session_that_names_a_different_stage() {
        let session = running_session("build");
        assert!(!heartbeat_applies_to(&session, "test"));
    }

    #[test]
    fn refuses_a_session_with_no_stage() {
        let mut session = Session::new();
        session.status = SessionStatus::Running;
        assert!(!heartbeat_applies_to(&session, "build"));
    }

    /// A stage that crashed and retried leaves the old session on disk with
    /// `stage_id` still set. A heartbeat from the live session must not revive
    /// the corpse's `last_active`, or `loom sessions list` reports two running
    /// sessions for one stage.
    #[test]
    fn refuses_a_terminal_session_for_the_same_stage() {
        let mut session = running_session("build");
        session.status = SessionStatus::Completed;
        assert!(!heartbeat_applies_to(&session, "build"));
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
