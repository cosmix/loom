//! When a silent session is reported, and when that report becomes evidence
//! of death rather than a warning.
//!
//! A hung report is suppressed after the first one so a stuck session does not
//! reprint its warning every five seconds. That suppression used to be
//! absolute, which left the report purely advisory: the one event a silence
//! ever produced arrived the moment the budget was first crossed, when a slow
//! agent and a dead one still look identical, and nothing could act on it.
//!
//! So exactly one further report is allowed, once the silence has run to
//! [`STALL_ESCALATION_MULTIPLIER`] times the stage's response budget. A
//! session that wakes up clears both latches on its next heartbeat, which is
//! what keeps a blip from ever reaching the second report: the elapsed time
//! restarts from zero with it.
//!
//! # Judges are measured here too, and reported differently
//!
//! An adjudication session is silent in the same observable way and needs a
//! different answer. It does not own the stage it works on, so there is
//! nothing to hand off and re-queue: a stalled judge is closed, and the stage
//! it left in `NeedsAdjudication` is re-judged on the next poll under the
//! dispute's own attempt budget. It is also latched once rather than twice —
//! the first report already ends the session, so there is no second line to
//! escalate to.

use chrono::{DateTime, Utc};
use std::time::Duration;

use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::Stage;

use super::config::MonitorConfig;
use super::detection::Detection;
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::{HeartbeatStatus, HeartbeatWatcher};
use super::parked::stage_looks_finished;

/// The multiple of a stage's response budget at which a still-silent session
/// stops being a warning and starts being evidence its agent is gone.
///
/// Three budgets, not two: the first report already costs the agent one full
/// budget, so a session that answers once more and then goes quiet again
/// restarts well below this line and cannot trip it.
const STALL_ESCALATION_MULTIPLIER: u64 = 3;

/// Whether a silence has run long enough to be treated as a dead session
/// rather than a slow one.
///
/// A stage may declare `subagent_timeout_secs: 0`, and nothing rejects it. A
/// zero budget calls every session hung on its first poll, which was harmless
/// while the report was advisory and would be a session killed on sight now.
/// So a stage that declares no real budget gets warnings and nothing else.
pub(crate) fn is_stall_escalation(stale_duration_secs: u64, timeout_secs: u64) -> bool {
    timeout_secs > 0
        && stale_duration_secs >= timeout_secs.saturating_mul(STALL_ESCALATION_MULTIPLIER)
}

impl Detection {
    /// What one running session's silence is worth on this tick, if anything.
    ///
    /// The two kinds part company immediately: a judge is measured against its
    /// own heartbeat file and reported as
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

    /// A stage agent that has stopped heartbeating. Unchanged behaviour: the
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
        // Pass the session ID so a stale heartbeat left by a previous session
        // for the same stage does not flag this fresh session as hung.
        let status = heartbeat_watcher.check_session_hung(
            stage_id,
            &session.id,
            Duration::from_secs(timeout_secs),
        );
        let stale_duration_secs = match status {
            HeartbeatStatus::Hung {
                stale_duration_secs,
            } => stale_duration_secs,
            // Answering again, so the next silence starts from the first
            // warning.
            HeartbeatStatus::Healthy => {
                self.clear_hung_report(&session.id);
                return None;
            }
            // Normal for a session that has not reached its first tool call.
            HeartbeatStatus::NoHeartbeat => return None,
        };

        // A dead PID is a crash, and crash detection in
        // `detect_session_changes` owns it, so only a live one is reported hung.
        if !self.hung_report_due(&session.id, stale_duration_secs, timeout_secs)
            || !matches!(handlers.check_session_alive(session), Ok(Some(true)))
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
        self.record_hung_report(&session.id, stale_duration_secs, timeout_secs);
        Some(event)
    }

    /// A judge that is still alive and has stopped working.
    ///
    /// Measured from its own heartbeat when it has written one, and from its
    /// spawn otherwise — a judge that never reached a tool call is exactly the
    /// case this watchdog exists for, since it is the shape a permission
    /// prompt or an API outage takes.
    fn adjudicator_stall_event(
        &mut self,
        session: &Session,
        stage_id: &str,
        heartbeat_watcher: &HeartbeatWatcher,
        timeout_secs: u64,
        handlers: &Handlers,
    ) -> Option<MonitorEvent> {
        // A stage may declare `subagent_timeout_secs: 0` and nothing rejects
        // it. Zero would close every judge on its first poll, so a stage that
        // declares no real budget gets no judge watchdog — the same rule
        // `is_stall_escalation` applies to stage agents, for the same reason.
        if timeout_secs == 0 || self.reported_stalled_judges.contains(&session.id) {
            return None;
        }
        let idle_for = Utc::now()
            .signed_duration_since(judge_last_activity(session, stage_id, heartbeat_watcher))
            .num_seconds();
        let Ok(stale_duration_secs) = u64::try_from(idle_for) else {
            return None;
        };
        // A judge whose process is already gone is a crash or an ordinary
        // exit, both of which `detect_vanished_process` owns.
        if stale_duration_secs <= timeout_secs
            || !matches!(handlers.check_session_alive(session), Ok(Some(true)))
        {
            return None;
        }
        self.reported_stalled_judges.insert(session.id.clone());
        Some(MonitorEvent::AdjudicatorStalled {
            session_id: session.id.clone(),
            stage_id: stage_id.to_string(),
            stale_duration_secs,
            timeout_secs,
        })
    }

    /// Drop the stall latch for every judge that is no longer running, so the
    /// next judge on the same stage is measured from scratch. Keyed by session
    /// id, so this only ever forgets sessions that have already ended.
    pub(super) fn clear_finished_judge_latches(&mut self, sessions: &[Session]) {
        let running: std::collections::HashSet<_> = sessions
            .iter()
            .filter(|session| session.status == SessionStatus::Running)
            .map(|session| session.id.as_str())
            .collect();
        self.reported_stalled_judges
            .retain(|session_id| running.contains(session_id.as_str()));
    }

    /// Whether this silence is worth an event: the first one for the session,
    /// then one more when it crosses the escalation line.
    pub(super) fn hung_report_due(
        &self,
        session_id: &str,
        stale_duration_secs: u64,
        timeout_secs: u64,
    ) -> bool {
        if !self.reported_hung_sessions.contains(session_id) {
            return true;
        }
        is_stall_escalation(stale_duration_secs, timeout_secs)
            && !self.escalated_hung_sessions.contains(session_id)
    }

    /// Record an emitted report so it is not repeated at every poll.
    pub(super) fn record_hung_report(
        &mut self,
        session_id: &str,
        stale_duration_secs: u64,
        timeout_secs: u64,
    ) {
        self.reported_hung_sessions.insert(session_id.to_string());
        if is_stall_escalation(stale_duration_secs, timeout_secs) {
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

/// When a judge was last seen working.
///
/// A heartbeat naming a DIFFERENT session is a previous judge's, left on the
/// stage's adjudication key after that judge was closed. It says nothing about
/// this one, so this one is measured from its spawn instead — the same rule
/// [`HeartbeatWatcher::check_session_hung`] applies to stage agents.
fn judge_last_activity(
    session: &Session,
    stage_id: &str,
    heartbeat_watcher: &HeartbeatWatcher,
) -> DateTime<Utc> {
    heartbeat_watcher
        .judge_heartbeat(stage_id)
        .filter(|heartbeat| heartbeat.session_id == session.id)
        .map_or(session.created_at, |heartbeat| heartbeat.timestamp)
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
    let last_activity = heartbeat_watcher
        .get_heartbeat(stage_id)
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
        assert!(detection.hung_report_due("session-1", 310, 300));
        detection.record_hung_report("session-1", 310, 300);

        assert!(!is_stall_escalation(310, 300));
        let (reported, escalated) = detection.hung_latches();
        assert!(reported.contains("session-1"));
        assert!(!escalated.contains("session-1"));
    }

    /// Every poll in between must stay quiet, or the warning prints every
    /// five seconds for as long as the session is stuck.
    #[test]
    fn reports_between_the_two_lines_are_suppressed() {
        let mut detection = Detection::new();
        detection.record_hung_report("session-1", 310, 300);

        assert!(!detection.hung_report_due("session-1", 600, 300));
        assert!(!detection.hung_report_due("session-1", 899, 300));
    }

    /// One further report, once, at the escalation line — the event the
    /// recovery path acts on.
    #[test]
    fn the_escalating_report_is_due_exactly_once() {
        let mut detection = Detection::new();
        detection.record_hung_report("session-1", 310, 300);

        assert!(detection.hung_report_due("session-1", 900, 300));
        detection.record_hung_report("session-1", 900, 300);
        assert!(!detection.hung_report_due("session-1", 5_000, 300));
    }

    /// A session that answers again cancels the escalation by itself: the
    /// clock it is measured against restarts with the heartbeat.
    #[test]
    fn a_fresh_heartbeat_returns_the_session_to_the_first_warning() {
        let mut detection = Detection::new();
        detection.record_hung_report("session-1", 900, 300);
        detection.clear_hung_report("session-1");

        assert!(detection.hung_report_due("session-1", 310, 300));
        let (reported, escalated) = detection.hung_latches();
        assert!(!reported.contains("session-1"));
        assert!(!escalated.contains("session-1"));
    }

    /// The line is the stage's OWN budget times three, so a stage that
    /// declares a long one is judged against it and not against the default.
    #[test]
    fn escalation_scales_with_the_stages_own_budget() {
        assert!(is_stall_escalation(10_800, 3_600));
        assert!(!is_stall_escalation(10_799, 3_600));
    }

    /// A stage declaring a zero budget calls its session hung on the first
    /// poll. Warn about it, never kill on it.
    #[test]
    fn a_zero_budget_never_escalates() {
        assert!(!is_stall_escalation(0, 0));
        assert!(!is_stall_escalation(86_400, 0));
    }
}
