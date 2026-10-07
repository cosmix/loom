use std::cell::Cell;
use std::time::{Duration, Instant};

use super::*;
use crate::fs::inbox::{append_ledger, LedgerOutcome, LedgerRecord, LedgerState};
use crate::relay::{new_request_id, RequestKind};

fn record(
    id: &str,
    state: Option<LedgerState>,
    outcome: Option<LedgerOutcome>,
    reason: Option<&str>,
) -> LedgerRecord {
    LedgerRecord {
        id: id.to_string(),
        kind: RequestKind::Commit,
        state,
        outcome,
        reason: reason.map(str::to_string),
        at: chrono::Utc::now(),
    }
}

fn wait_once(status: ReportedStatus) -> Waited {
    let mut resolve = || Ok(status.clone());
    let mut now = Instant::now;
    let mut sleep = |_| panic!("settled status must not sleep");
    wait_for(&mut resolve, Duration::from_secs(1), &mut now, &mut sleep).unwrap()
}

#[test]
fn wait_returns_at_once_when_the_request_is_already_applied() {
    let sleeps = Cell::new(0);
    let mut resolve = || Ok(ReportedStatus::Inbox(RequestStatus::Applied));
    let mut now = Instant::now;
    let mut sleep = |_| sleeps.set(sleeps.get() + 1);

    let waited = wait_for(&mut resolve, Duration::from_secs(1), &mut now, &mut sleep).unwrap();

    assert_eq!(
        waited,
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::Applied))
    );
    assert_eq!(sleeps.get(), 0);
}

#[test]
fn wait_polls_until_the_daemon_applies() {
    let base = Instant::now();
    let elapsed = Cell::new(Duration::ZERO);
    let sleeps = Cell::new(0);
    let mut statuses = [
        RequestStatus::RelayedAwaitingDaemon,
        RequestStatus::Applying,
        RequestStatus::Applied,
    ]
    .into_iter();
    let mut resolve = || Ok(ReportedStatus::Inbox(statuses.next().unwrap()));
    let mut now = || base + elapsed.get();
    let mut sleep = |duration| {
        assert_eq!(duration, POLL_INTERVAL);
        sleeps.set(sleeps.get() + 1);
        elapsed.set(elapsed.get() + duration);
    };

    let waited = wait_for(&mut resolve, Duration::from_secs(3), &mut now, &mut sleep).unwrap();

    assert_eq!(
        waited,
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::Applied))
    );
    assert_eq!(sleeps.get(), 2);
}

#[test]
fn wait_reports_a_refusal_with_its_reason() {
    let waited = wait_once(ReportedStatus::Inbox(RequestStatus::Refused {
        reason: "signing failed".to_string(),
    }));

    let error = outcome_line("request-1", waited, 1, None).unwrap_err();

    assert!(error.to_string().contains("signing failed"));
}

#[test]
fn wait_times_out_naming_the_request_and_the_deadline() {
    let base = Instant::now();
    let elapsed = Cell::new(Duration::ZERO);
    let mut resolve = || Ok(ReportedStatus::Inbox(RequestStatus::Applying));
    let mut now = || base + elapsed.get();
    let mut sleep = |duration| {
        assert!(duration <= POLL_INTERVAL);
        elapsed.set(elapsed.get() + duration);
    };

    let waited = wait_for(&mut resolve, Duration::from_secs(3), &mut now, &mut sleep).unwrap();
    let error = outcome_line("request-1", waited, 3, None).unwrap_err();

    assert_eq!(
        error.to_string(),
        "request request-1 still pending after 3s"
    );
    assert_eq!(elapsed.get(), Duration::from_secs(3));
}

#[test]
fn wait_ends_at_once_on_not_found_and_on_a_ticket_the_relay_never_took() {
    for status in [
        ReportedStatus::Inbox(RequestStatus::NotFound),
        ReportedStatus::PendingRelay,
    ] {
        let waited = wait_once(status);
        let error = outcome_line("request-1", waited, 1, None).unwrap_err();

        assert!(error.to_string().contains("request request-1"));
    }
}

#[test]
fn a_chained_wait_on_an_unrelayed_ticket_says_to_wait_again_not_to_recreate_it() {
    let waited = wait_once(ReportedStatus::PendingRelay);

    let message = outcome_line("request-1", waited, 90, None)
        .unwrap_err()
        .to_string();

    assert!(message.starts_with("request request-1 is not relayed yet"));
    assert!(
        message.contains("`loom request status request-1 --wait 90` again as its own Bash call")
    );
    assert!(message.contains("do not run the command that created the request again"));
    assert!(!message.contains("never relayed"));
}

#[test]
fn wait_treats_unknown_after_restart_as_an_error() {
    let waited = wait_once(ReportedStatus::Inbox(RequestStatus::UnknownAfterRestart));

    let error = outcome_line("request-1", waited, 1, None).unwrap_err();

    assert!(error.to_string().contains("unknown after a daemon restart"));
}

#[test]
fn the_applied_line_carries_the_commit_note_from_the_ledger() {
    let root = tempfile::tempdir().unwrap();
    let id = new_request_id();
    let commit_id = "0123456789012345678901234567890123456789";
    let note = format!("committed {commit_id}");
    append_ledger(
        root.path(),
        "session-1",
        &record(&id, None, Some(LedgerOutcome::Applied), Some(&note)),
    )
    .unwrap();

    let line = outcome_line(
        &id,
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::Applied)),
        1,
        applied_note(root.path(), Some("session-1"), &id).unwrap(),
    )
    .unwrap();

    assert_eq!(line, format!("{id}: applied: {note}"));
}

#[test]
fn the_applied_line_without_a_note_is_plain() {
    let line = outcome_line(
        "request-1",
        Waited::Settled(ReportedStatus::Inbox(RequestStatus::Applied)),
        1,
        None,
    )
    .unwrap();

    assert_eq!(line, "request-1: applied");
}

#[test]
fn applied_note_takes_the_latest_row() {
    let root = tempfile::tempdir().unwrap();
    let id = new_request_id();
    append_ledger(
        root.path(),
        "session-1",
        &record(&id, Some(LedgerState::Applying), None, None),
    )
    .unwrap();
    append_ledger(
        root.path(),
        "session-1",
        &record(
            &id,
            None,
            Some(LedgerOutcome::Applied),
            Some("committed 0123456789012345678901234567890123456789"),
        ),
    )
    .unwrap();

    let note = applied_note(root.path(), Some("session-1"), &id).unwrap();

    assert_eq!(
        note.as_deref(),
        Some("committed 0123456789012345678901234567890123456789")
    );
}

#[test]
fn applied_note_scans_every_inbox_session_without_a_session_argument() {
    let root = tempfile::tempdir().unwrap();
    let id = new_request_id();
    let other_id = new_request_id();
    append_ledger(
        root.path(),
        "session-1",
        &record(&other_id, Some(LedgerState::Applying), None, None),
    )
    .unwrap();
    append_ledger(
        root.path(),
        "session-2",
        &record(
            &id,
            None,
            Some(LedgerOutcome::Applied),
            Some("committed 0123456789012345678901234567890123456789"),
        ),
    )
    .unwrap();

    let note = applied_note(root.path(), None, &id).unwrap();

    assert_eq!(
        note.as_deref(),
        Some("committed 0123456789012345678901234567890123456789")
    );
}
