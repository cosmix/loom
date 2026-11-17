use super::*;
use crate::fs::inbox::{append_ledger, LedgerRecord, LedgerState};
use crate::relay::{new_request_id, RequestKind};
use chrono::Utc;

#[test]
fn a_ticket_without_inbox_evidence_is_pending_relay() {
    let work_dir = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let id = new_request_id();
    std::fs::write(scratch.path().join(format!("{id}.req")), b"{}").unwrap();

    let status = resolve_status(
        work_dir.path(),
        Some("session-1"),
        Some(scratch.path()),
        &id,
    )
    .unwrap();
    assert_eq!(status, ReportedStatus::PendingRelay);
}

#[test]
fn accepted_inbox_evidence_wins_over_a_leftover_ticket() {
    for (state, outcome, expected) in [
        (Some(LedgerState::Applying), None, RequestStatus::Applying),
        (None, Some(LedgerOutcome::Applied), RequestStatus::Applied),
        (
            None,
            Some(LedgerOutcome::Refused),
            RequestStatus::Refused {
                reason: "rejected".into(),
            },
        ),
    ] {
        let work_dir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let id = new_request_id();
        std::fs::write(scratch.path().join(format!("{id}.req")), b"{}").unwrap();
        append_ledger(
            work_dir.path(),
            "session-1",
            &LedgerRecord {
                id: id.clone(),
                kind: RequestKind::Memory,
                state,
                outcome,
                reason: Some("rejected".into()),
                at: Utc::now(),
            },
        )
        .unwrap();
        for session in [Some("session-1"), None] {
            assert_eq!(
                resolve_status(work_dir.path(), session, Some(scratch.path()), &id).unwrap(),
                ReportedStatus::Inbox(expected.clone())
            );
        }
    }
}

#[test]
fn a_ticket_does_not_bypass_explicit_session_validation() {
    let work_dir = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let id = new_request_id();
    std::fs::write(scratch.path().join(format!("{id}.req")), b"{}").unwrap();
    assert!(resolve_status(
        work_dir.path(),
        Some("../invalid"),
        Some(scratch.path()),
        &id
    )
    .is_err());
}

#[test]
fn an_explicit_session_is_looked_up_directly() {
    let work_dir = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let id = new_request_id();
    let dir = work_dir.path().join("inbox").join("session-1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();
    std::fs::write(scratch.path().join(format!("{id}.req")), b"{}").unwrap();

    let status = resolve_status(
        work_dir.path(),
        Some("session-1"),
        Some(scratch.path()),
        &id,
    )
    .unwrap();
    assert_eq!(
        status,
        ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon)
    );
}

#[test]
fn without_a_session_every_inbox_is_scanned() {
    let work_dir = tempfile::tempdir().unwrap();
    let id = new_request_id();
    std::fs::create_dir_all(work_dir.path().join("inbox").join("session-1")).unwrap();
    let dir = work_dir.path().join("inbox").join("session-2");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();

    let status = resolve_status(work_dir.path(), None, None, &id).unwrap();
    assert_eq!(
        status,
        ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon)
    );
}

#[test]
fn not_found_when_no_session_has_it() {
    let work_dir = tempfile::tempdir().unwrap();
    let id = new_request_id();
    let status = resolve_status(work_dir.path(), None, None, &id).unwrap();
    assert_eq!(status, ReportedStatus::Inbox(RequestStatus::NotFound));
}

#[test]
fn a_work_dir_symlink_resolves_once_canonicalized() {
    let base = tempfile::tempdir().unwrap();
    let real = base.path().join("real");
    let id = new_request_id();
    let dir = real.join("inbox").join("session-1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();
    let link_parent = base.path().join("wt").join(".loom");
    std::fs::create_dir_all(&link_parent).unwrap();
    let link = link_parent.join("work");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    assert!(resolve_status(&link, Some("session-1"), None, &id).is_err());
    assert_eq!(
        resolve_status(&canonical_root(&link), Some("session-1"), None, &id).unwrap(),
        ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon)
    );
}

#[test]
fn an_invalid_id_is_refused() {
    let work_dir = tempfile::tempdir().unwrap();
    assert!(resolve_status(work_dir.path(), None, None, "not-hex").is_err());
}

#[test]
fn format_status_reports_not_found_with_exit_1() {
    let (message, not_found) = format_status(&ReportedStatus::Inbox(RequestStatus::NotFound));
    assert_eq!(message, "not found");
    assert!(not_found);
}

#[test]
fn format_status_reports_every_other_state_with_exit_0() {
    let cases = [
        ReportedStatus::PendingRelay,
        ReportedStatus::Inbox(RequestStatus::RelayedAwaitingDaemon),
        ReportedStatus::Inbox(RequestStatus::Applying),
        ReportedStatus::Inbox(RequestStatus::Applied),
        ReportedStatus::Inbox(RequestStatus::Refused {
            reason: "bad kind".to_string(),
        }),
        ReportedStatus::Inbox(RequestStatus::UnknownAfterRestart),
    ];
    for case in cases {
        let (_, not_found) = format_status(&case);
        assert!(!not_found, "{case:?} should exit 0");
    }
}

#[test]
fn a_settled_pending_relay_tells_a_lost_control_request_to_be_rerun() {
    let error = outcome_line(
        "request-1",
        Waited::Settled(ReportedStatus::PendingRelay),
        1,
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("is never relayed later"), "{error}");
    assert!(error.contains("do not run the command that created the request again"));
}
