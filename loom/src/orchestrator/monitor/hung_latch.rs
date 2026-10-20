//! When a silent session is reported, and when that report becomes evidence
//! of death rather than a warning.
//!
//! A hung report is suppressed after the first one so a stuck session does not
//! reprint its warning every five seconds. That suppression used to be
//! absolute, which left the report purely advisory: the one event a silence
//! ever produced arrived the moment the budget was first crossed, when a slow
//! agent and a dead one still look identical, and nothing could act on it.
//!
//! So exactly one further report is allowed, once the silence reaches the
//! line [`is_stall_escalation`] draws: three response budgets, or longer while
//! the session's pinned completion runs. A
//! session that wakes up clears both latches on its next heartbeat, which is
//! what keeps a blip from ever reaching the second report: the elapsed time
//! restarts from zero with it.
//!
//! Judges are silent in the same observable way and are dispatched from here
//! to [`super::judge_watchdog`], which measures and reports them differently.

use std::time::Duration;

use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::Stage;

use super::config::MonitorConfig;
use super::detection::Detection;
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::{HeartbeatStatus, HeartbeatWatcher};
use super::never_worked::silence_without_heartbeat;
use super::parked::stage_looks_finished;
use super::stall_line::{is_stall_escalation, running_completion_line};

impl Detection {
    /// What one running session's silence is worth on this tick, if anything.
    ///
    /// The two kinds part company immediately: a judge is measured against its
    /// own heartbeat file and its own running commands, reported as
    /// [`MonitorEvent::AdjudicatorStalled`], and never falls through to the
    /// stage-agent path, whose heartbeat it does not write and whose
    /// recovery — handing the stage off to a successor — would be wrong for it.
    pub(super) fn session_silence_event(
        &mut self,
        session: &Session,
        stages: &[Stage],
        heartbeat_watcher: &HeartbeatWatcher,
        config: &MonitorConfig,
        handlers: &Handlers,
    ) -> Option<MonitorEvent> {
        if session.status != SessionStatus::Running {
            return None;
        }
        let stage_id = session.stage_id.as_deref()?;

        // Resolve this stage's response budget. One watcher serves every
        // stage, so the threshold is passed per check rather than held on
        // the watcher.
        let timeout_secs = stages
            .iter()
            .find(|s| s.id == *stage_id)
            .map(|s| s.effective_subagent_timeout_secs())
            .unwrap_or_else(|| config.hung_timeout.as_secs());

        if session.session_type == SessionType::Adjudication {
            return self.adjudicator_stall_event(
                session,
                stage_id,
                stages,
                heartbeat_watcher,
                timeout_secs,
                handlers,
            );
        }
        self.stage_silence_event(
            session,
            stage_id,
            stages,
            heartbeat_watcher,
            timeout_secs,
            handlers,
        )
    }

    /// A stage agent that has stopped heartbeating, or never started: the
    /// first silence past the budget is reported, then one more at the
    /// escalation line.
    fn stage_silence_event(
        &mut self,
        session: &Session,
        stage_id: &str,
        stages: &[Stage],
        heartbeat_watcher: &HeartbeatWatcher,
        timeout_secs: u64,
        handlers: &Handlers,
    ) -> Option<MonitorEvent> {
        let stale_duration_secs =
            self.stage_silence_secs(session, stage_id, heartbeat_watcher, timeout_secs)?;
        let completion_line = running_completion_line(
            heartbeat_watcher.get_heartbeat(stage_id),
            &session.id,
            stages.iter().find(|stage| stage.id == stage_id),
        );
        // A dead PID is a crash, and crash detection in
        // `detect_session_changes` owns it, so only a live one is reported hung.
        if !self.hung_report_due(
            &session.id,
            stale_duration_secs,
            timeout_secs,
            completion_line,
        ) || !matches!(handlers.check_session_alive(session), Ok(Some(true)))
        {
            return None;
        }
        let event = hung_event(
            session,
            stage_id,
            stages,
            heartbeat_watcher,
            stale_duration_secs,
            timeout_secs,
        );
        self.record_hung_report(
            &session.id,
            stale_duration_secs,
            timeout_secs,
            completion_line,
        );
        Some(event)
    }

    /// Silence of a stage session, or `None` when it answers (resetting its latch).
    fn stage_silence_secs(
        &mut self,
        session: &Session,
        stage_id: &str,
        heartbeat_watcher: &HeartbeatWatcher,
        timeout_secs: u64,
    ) -> Option<u64> {
        // The session ID keeps a previous session's stale heartbeat from flagging this one.
        let status = heartbeat_watcher.check_session_hung(
            stage_id,
            &session.id,
            Duration::from_secs(timeout_secs),
        );
        match status {
            HeartbeatStatus::Hung {
                stale_duration_secs,
                ..
            } => Some(stale_duration_secs),
            // Answering again: the next silence starts from the first warning.
            HeartbeatStatus::Healthy => {
                self.clear_hung_report(&session.id);
                None
            }
            // Measured from spawn, so a session that never reaches a tool call is reported.
            HeartbeatStatus::NoHeartbeat => {
                silence_without_heartbeat(session, heartbeat_watcher.now(), timeout_secs)
            }
        }
    }

    /// Whether this silence is worth an event: the first one for the session,
    /// then one more when it crosses the escalation line.
    pub(super) fn hung_report_due(
        &self,
        session_id: &str,
        stale_duration_secs: u64,
        timeout_secs: u64,
        completion_line: Option<u64>,
    ) -> bool {
        if !self.reported_hung_sessions.contains(session_id) {
            return true;
        }
        is_stall_escalation(stale_duration_secs, timeout_secs, completion_line)
            && !self.escalated_hung_sessions.contains(session_id)
    }

    /// Record an emitted report so it is not repeated at every poll.
    pub(super) fn record_hung_report(
        &mut self,
        session_id: &str,
        stale_duration_secs: u64,
        timeout_secs: u64,
        completion_line: Option<u64>,
    ) {
        self.reported_hung_sessions.insert(session_id.to_string());
        if is_stall_escalation(stale_duration_secs, timeout_secs, completion_line) {
            self.escalated_hung_sessions.insert(session_id.to_string());
        }
    }

    /// Forget a session's reports. Called for a fresh heartbeat and for a
    /// healthy one: the session is answering again, so the next silence is a
    /// new episode that starts from the first warning.
    pub(super) fn clear_hung_report(&mut self, session_id: &str) {
        self.reported_hung_sessions.remove(session_id);
        self.escalated_hung_sessions.remove(session_id);
    }

    /// Both latches, so a test can say which line a session has crossed.
    #[cfg(test)]
    pub(super) fn hung_latches(
        &self,
    ) -> (
        &std::collections::HashSet<String>,
        &std::collections::HashSet<String>,
    ) {
        (&self.reported_hung_sessions, &self.escalated_hung_sessions)
    }
}

/// Build the `SessionHung` event for a session whose heartbeat has gone stale.
///
/// Kept out of the detection loop so the classification has somewhere to live,
/// and so the two git probes behind [`stage_looks_finished`] are visibly paid
/// once per hung REPORT rather than once per poll.
pub(super) fn hung_event(
    session: &Session,
    stage_id: &str,
    stages: &[Stage],
    heartbeat_watcher: &HeartbeatWatcher,
    stale_duration_secs: u64,
    timeout_secs: u64,
) -> MonitorEvent {
    // A previous session's activity must not describe this one.
    let last_activity = heartbeat_watcher
        .get_heartbeat(stage_id)
        .filter(|hb| hb.session_id == session.id)
        .and_then(|hb| hb.activity.clone());

    let finished_without_completing = stages
        .iter()
        .find(|s| s.id == stage_id)
        .is_some_and(|stage| stage_looks_finished(session, stage));

    MonitorEvent::SessionHung {
        session_id: session.id.clone(),
        stage_id: Some(stage_id.to_string()),
        stale_duration_secs,
        timeout_secs,
        last_activity,
        finished_without_completing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first silence past the budget is a warning, and only a warning:
    /// nothing yet tells a slow agent apart from a dead one.
    #[test]
    fn the_first_report_is_due_and_does_not_escalate() {
        let mut detection = Detection::new();
        assert!(detection.hung_report_due("session-1", 310, 300, None));
        detection.record_hung_report("session-1", 310, 300, None);

        assert!(!is_stall_escalation(310, 300, None));
        let (reported, escalated) = detection.hung_latches();
        assert!(reported.contains("session-1"));
        assert!(!escalated.contains("session-1"));
    }

    /// Every poll in between must stay quiet, or the warning prints every
    /// five seconds for as long as the session is stuck.
    #[test]
    fn reports_between_the_two_lines_are_suppressed() {
        let mut detection = Detection::new();
        detection.record_hung_report("session-1", 310, 300, None);

        assert!(!detection.hung_report_due("session-1", 600, 300, None));
        assert!(!detection.hung_report_due("session-1", 899, 300, None));
    }

    /// One further report, once, at the escalation line — the event the
    /// recovery path acts on.
    #[test]
    fn the_escalating_report_is_due_exactly_once() {
        let mut detection = Detection::new();
        detection.record_hung_report("session-1", 310, 300, None);

        assert!(detection.hung_report_due("session-1", 900, 300, None));
        detection.record_hung_report("session-1", 900, 300, None);
        assert!(!detection.hung_report_due("session-1", 5_000, 300, None));
    }

    /// A session that answers again cancels the escalation by itself: the
    /// clock it is measured against restarts with the heartbeat.
    #[test]
    fn a_fresh_heartbeat_returns_the_session_to_the_first_warning() {
        let mut detection = Detection::new();
        detection.record_hung_report("session-1", 900, 300, None);
        detection.clear_hung_report("session-1");

        assert!(detection.hung_report_due("session-1", 310, 300, None));
        let (reported, escalated) = detection.hung_latches();
        assert!(!reported.contains("session-1"));
        assert!(!escalated.contains("session-1"));
    }

    /// While the pinned completion runs, the escalating report waits for the
    /// completion's own line rather than three budgets.
    #[test]
    fn a_running_completion_escalates_only_at_its_own_line() {
        let mut detection = Detection::new();
        detection.record_hung_report("session-1", 310, 300, Some(2_400));

        assert!(!detection.hung_report_due("session-1", 900, 300, Some(2_400)));
        assert!(detection.hung_report_due("session-1", 2_400, 300, Some(2_400)));
    }
}
