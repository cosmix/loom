//! Whether a stage session never started work.
//!
//! A stage agent that never reaches its first tool call leaves one of two
//! traces: no heartbeat at all, or only the one `session-start.sh` writes
//! (`last_tool: null`). Waiting three response budgets for it and then
//! re-queueing it charges the stage a recovery for an attempt that did
//! nothing, and whatever stopped it (a session that is not logged in, a
//! prompt nobody answers) stops the next attempt too. So such a session is
//! reported once its own budget runs out, and the handler parks its stage on
//! that first report.
//!
//! `session-start.sh` also fires on compaction and resume and rewrites the
//! heartbeat in the same shape, so the session record's token count decides:
//! a session that has worked carries the tokens its earlier tool heartbeats
//! reported, and one that never worked still reads zero.
//!
//! Only `SessionType::Stage` sessions are judged here. Contract, Knowledge,
//! Merge, BaseConflict and Adjudication sessions keep their own handling.

use chrono::{DateTime, Utc};

use crate::models::session::{Session, SessionType};

use super::heartbeat::Heartbeat;
use super::hung_latch::is_stall_escalation;

/// Whether `session` can be judged at all: a stage agent with a real budget
/// that has not yet reported any context.
fn judged(session: &Session, budget_secs: u64) -> bool {
    budget_secs > 0 && session.session_type == SessionType::Stage && session.context_tokens == 0
}

/// How long a stage session that has written no heartbeat of its own has been
/// silent since its spawn, once that silence has reached its budget.
pub(crate) fn silence_without_heartbeat(
    session: &Session,
    now: DateTime<Utc>,
    budget_secs: u64,
) -> Option<u64> {
    if !judged(session, budget_secs) {
        return None;
    }
    let silent_secs =
        u64::try_from(now.signed_duration_since(session.created_at).num_seconds()).ok()?;
    (silent_secs >= budget_secs).then_some(silent_secs)
}

/// Whether `session` never started work: its own heartbeat names no tool and
/// no subagent, or it has none past its budget. A heartbeat naming another
/// session is a previous attempt's and says nothing about this one.
pub(crate) fn never_worked(
    session: &Session,
    heartbeat: Option<&Heartbeat>,
    now: DateTime<Utc>,
    budget_secs: u64,
) -> bool {
    if !judged(session, budget_secs) {
        return false;
    }
    match heartbeat.filter(|heartbeat| heartbeat.session_id == session.id) {
        Some(heartbeat) => heartbeat.last_tool.is_none() && !heartbeat.subagent,
        None => silence_without_heartbeat(session, now, budget_secs).is_some(),
    }
}

/// Whether a silence is evidence enough to act on. A session that never
/// started work is acted on at its budget, since waiting cannot help it; any
/// other session at [`is_stall_escalation`]'s line.
pub(crate) fn is_escalation(
    stale_duration_secs: u64,
    timeout_secs: u64,
    never_worked: bool,
) -> bool {
    if never_worked {
        return timeout_secs > 0 && stale_duration_secs >= timeout_secs;
    }
    is_stall_escalation(stale_duration_secs, timeout_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUDGET_SECS: u64 = 300;

    fn stage_session(now: DateTime<Utc>, age_secs: i64) -> Session {
        let mut session = Session::new();
        session.id = "session-1".to_string();
        session.created_at = now - chrono::Duration::seconds(age_secs);
        session
    }

    fn session_start(session_id: &str) -> Heartbeat {
        Heartbeat::new("stage-1".to_string(), session_id.to_string())
            .with_activity("Session started".to_string())
    }

    #[test]
    fn its_own_session_start_heartbeat_alone_is_never_worked() {
        let now = Utc::now();
        let session = stage_session(now, 10);
        assert!(never_worked(
            &session,
            Some(&session_start("session-1")),
            now,
            BUDGET_SECS
        ));
    }

    #[test]
    fn its_own_tool_heartbeat_is_work() {
        let now = Utc::now();
        let session = stage_session(now, 10);
        let heartbeat = session_start("session-1").with_last_tool("Bash".to_string());
        assert!(!never_worked(&session, Some(&heartbeat), now, BUDGET_SECS));
    }

    #[test]
    fn a_subagent_heartbeat_is_work() {
        let now = Utc::now();
        let session = stage_session(now, 10);
        let mut heartbeat = session_start("session-1");
        heartbeat.subagent = true;
        assert!(!never_worked(&session, Some(&heartbeat), now, BUDGET_SECS));
    }

    #[test]
    fn another_sessions_heartbeat_is_measured_from_spawn() {
        let now = Utc::now();
        let previous = session_start("old-session").with_last_tool("Bash".to_string());
        let young = stage_session(now, 60);
        let old = stage_session(now, BUDGET_SECS as i64 + 10);
        assert!(!never_worked(&young, Some(&previous), now, BUDGET_SECS));
        assert!(never_worked(&old, Some(&previous), now, BUDGET_SECS));
    }

    #[test]
    fn reported_context_is_work() {
        let now = Utc::now();
        let mut session = stage_session(now, BUDGET_SECS as i64 + 10);
        session.context_tokens = 12_000;
        assert!(!never_worked(
            &session,
            Some(&session_start("session-1")),
            now,
            BUDGET_SECS
        ));
        assert!(!never_worked(&session, None, now, BUDGET_SECS));
    }

    #[test]
    fn a_merge_session_is_not_judged() {
        let now = Utc::now();
        let mut session = stage_session(now, BUDGET_SECS as i64 + 10);
        session.session_type = SessionType::Merge;
        assert!(!never_worked(&session, None, now, BUDGET_SECS));
        assert_eq!(silence_without_heartbeat(&session, now, BUDGET_SECS), None);
    }

    #[test]
    fn a_zero_budget_is_not_judged() {
        let now = Utc::now();
        let session = stage_session(now, 86_400);
        assert!(!never_worked(
            &session,
            Some(&session_start("session-1")),
            now,
            0
        ));
        assert_eq!(silence_without_heartbeat(&session, now, 0), None);
    }

    #[test]
    fn silence_without_a_heartbeat_counts_from_spawn() {
        let now = Utc::now();
        assert_eq!(
            silence_without_heartbeat(&stage_session(now, 299), now, BUDGET_SECS),
            None
        );
        assert_eq!(
            silence_without_heartbeat(&stage_session(now, 300), now, BUDGET_SECS),
            Some(300)
        );
    }

    #[test]
    fn never_worked_escalates_at_one_budget_and_work_at_three() {
        assert!(is_escalation(300, 300, true));
        assert!(!is_escalation(299, 300, true));
        assert!(!is_escalation(86_400, 0, true));
        assert!(!is_escalation(310, 300, false));
        assert!(is_escalation(900, 300, false));
    }
}
