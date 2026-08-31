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

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::detection::Detection;
use super::events::MonitorEvent;
use super::heartbeat::HeartbeatWatcher;
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
