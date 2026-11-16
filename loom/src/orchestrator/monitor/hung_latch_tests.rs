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
