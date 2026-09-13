//! The drain's own guarantees: refusals are outcomes, I/O failure leaves the
//! entry, dedupe and at-most-once, and entries that are not the relay's own
//! write are refused.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;

use chrono::{Duration, Utc};

use crate::fs::inbox::{append_ledger, LedgerOutcome, LedgerRecord, LedgerState};
use crate::models::dispute::verdict_file;
use crate::models::session::{SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::relay::{new_request_id, InboxEntry, RequestKind};

use super::session_pass::drain_session;
use super::test_support::{entry_for, fixture, memory_payload, payload_for, Fixture, STAGE};
use super::{run_pass, PassReport, Settle};

fn ledger_line(
    entry: &InboxEntry,
    state: Option<LedgerState>,
    outcome: Option<LedgerOutcome>,
) -> LedgerRecord {
    LedgerRecord {
        id: entry.id.clone(),
        kind: entry.kind,
        state,
        outcome,
        reason: None,
        at: Utc::now(),
    }
}

fn stage_session(fx: &Fixture) -> crate::models::session::Session {
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    record
}

#[test]
fn a_note_lands_in_the_journal_once_and_its_entry_is_gone() {
    let fx = fixture();
    let record = stage_session(&fx);
    let entry = fx.relay(
        &record,
        RequestKind::Memory,
        memory_payload("found a pattern"),
    );
    let mut host = fx.host(true);

    run_pass(&mut host, &fx.tick(Utc::now()));
    // The same entry arriving again changes nothing.
    fx.plant(&record.id, &format!("{}.json", entry.id), &entry.encode());
    run_pass(&mut host, &fx.tick(Utc::now()));

    assert_eq!(fx.journal_len(), 1);
    assert_eq!(
        fx.outcome(&record.id, &entry.id),
        Some(LedgerOutcome::Applied)
    );
    assert!(!fx
        .inbox(&record.id)
        .join(format!("{}.json", entry.id))
        .exists());
}

#[test]
fn a_refusal_is_a_ledger_outcome_not_an_error() {
    let fx = fixture();
    fx.stage(StageStatus::NeedsAdjudication, None);
    let judge = fx.record(SessionType::Adjudication, SessionStatus::Running);
    let entry = fx.relay(
        &judge,
        RequestKind::Memory,
        memory_payload("a judge's note"),
    );
    let mut host = fx.host(true);

    drain_session(&mut host, &judge.id, &judge, &mut PassReport::default())
        .expect("a refusal never surfaces as an error");

    assert_eq!(
        fx.outcome(&judge.id, &entry.id),
        Some(LedgerOutcome::Refused)
    );
    assert_eq!(fx.journal_len(), 0);
    assert!(!fx
        .inbox(&judge.id)
        .join(format!("{}.json", entry.id))
        .exists());
}

#[test]
fn an_io_failure_leaves_the_entry_for_the_next_tick() {
    let fx = fixture();
    let record = stage_session(&fx);
    let entry = fx.relay(
        &record,
        RequestKind::Memory,
        memory_payload("kept for later"),
    );
    let ledger = fx.inbox(&record.id).join("ledger.jsonl");
    std::fs::create_dir(&ledger).unwrap(); // the ledger can be neither read nor appended
    let mut host = fx.host(true);

    let result = drain_session(&mut host, &record.id, &record, &mut PassReport::default());

    assert!(result.is_err());
    assert!(fx
        .inbox(&record.id)
        .join(format!("{}.json", entry.id))
        .exists());
    assert_eq!(fx.journal_len(), 0);

    std::fs::remove_dir(&ledger).unwrap();
    drain_session(&mut host, &record.id, &record, &mut PassReport::default()).unwrap();
    assert_eq!(
        fx.outcome(&record.id, &entry.id),
        Some(LedgerOutcome::Applied)
    );
    assert_eq!(fx.journal_len(), 1);
}

#[test]
fn an_id_already_in_the_ledger_is_deleted_without_applying() {
    let fx = fixture();
    let record = stage_session(&fx);
    let entry = entry_for(&record, RequestKind::Memory, memory_payload("seen before"));
    append_ledger(
        &fx.work_dir,
        &record.id,
        &ledger_line(&entry, None, Some(LedgerOutcome::Applied)),
    )
    .unwrap();
    let path = fx.plant(&record.id, &format!("{}.json", entry.id), &entry.encode());

    run_pass(&mut fx.host(true), &fx.tick(Utc::now()));

    assert!(!path.exists());
    assert_eq!(fx.journal_len(), 0);
}

#[test]
fn an_interrupted_request_is_settled_unknown_and_never_applied() {
    let fx = fixture();
    let record = stage_session(&fx);
    let entry = entry_for(&record, RequestKind::Memory, memory_payload("half applied"));
    append_ledger(
        &fx.work_dir,
        &record.id,
        &ledger_line(&entry, Some(LedgerState::Applying), None),
    )
    .unwrap();
    fx.plant(&record.id, &format!("{}.json", entry.id), &entry.encode());
    let mut host = fx.host(true);

    run_pass(&mut host, &fx.tick(Utc::now()));
    let after_first = fx.ledger(&record.id).len();
    run_pass(&mut host, &fx.tick(Utc::now()));

    assert_eq!(
        fx.outcome(&record.id, &entry.id),
        Some(LedgerOutcome::UnknownAfterRestart)
    );
    assert_eq!(
        fx.ledger(&record.id).len(),
        after_first,
        "settled once, never again"
    );
    assert_eq!(fx.journal_len(), 0);
    assert!(!fx
        .inbox(&record.id)
        .join(format!("{}.json", entry.id))
        .exists());
}

#[test]
fn entries_naming_another_session_or_stage_are_refused() {
    let fx = fixture();
    let record = stage_session(&fx);
    let mut foreign_session = entry_for(&record, RequestKind::Memory, memory_payload("forged"));
    foreign_session.session_id = "session-someone-else".to_string();
    let mut foreign_stage = entry_for(&record, RequestKind::Memory, memory_payload("forged"));
    foreign_stage.stage_id = "other-stage".to_string();
    for entry in [&foreign_session, &foreign_stage] {
        fx.plant(&record.id, &format!("{}.json", entry.id), &entry.encode());
    }

    run_pass(&mut fx.host(true), &fx.tick(Utc::now()));

    assert_eq!(
        fx.outcome(&record.id, &foreign_session.id),
        Some(LedgerOutcome::Refused)
    );
    assert_eq!(
        fx.outcome(&record.id, &foreign_stage.id),
        Some(LedgerOutcome::Refused)
    );
    assert_eq!(fx.journal_len(), 0);
}

#[test]
fn a_malformed_entry_is_refused() {
    let fx = fixture();
    let record = stage_session(&fx);
    let named = new_request_id();
    fx.plant(
        &record.id,
        &format!("{named}.json"),
        br#"{"v":1,"kind":"memory","bogus":true}"#,
    );
    let garbage = new_request_id();
    let garbage_path = fx.plant(&record.id, &format!("{garbage}.json"), b"not json");

    let report = run_pass(&mut fx.host(true), &fx.tick(Utc::now()));

    assert_eq!(fx.outcome(&record.id, &named), Some(LedgerOutcome::Refused));
    assert!(!garbage_path.exists());
    assert!(report
        .settled
        .iter()
        .any(|(id, settle)| *id == garbage && matches!(settle, Settle::Refused(_))));
    assert_eq!(fx.journal_len(), 0);
}

#[test]
fn symlinked_hard_linked_and_fifo_entries_are_refused_unread() {
    let fx = fixture();
    let record = stage_session(&fx);
    let outside = fx.repo_root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let inbox = fx.inbox(&record.id);
    std::fs::create_dir_all(&inbox).unwrap();

    let linked = entry_for(&record, RequestKind::Memory, memory_payload("via symlink"));
    std::fs::write(outside.join("target.json"), linked.encode()).unwrap();
    std::os::unix::fs::symlink(
        outside.join("target.json"),
        inbox.join(format!("{}.json", linked.id)),
    )
    .unwrap();
    let hard = entry_for(
        &record,
        RequestKind::Memory,
        memory_payload("via hard link"),
    );
    std::fs::write(outside.join("hard.json"), hard.encode()).unwrap();
    std::fs::hard_link(
        outside.join("hard.json"),
        inbox.join(format!("{}.json", hard.id)),
    )
    .unwrap();
    let fifo = inbox.join(format!("{}.json", new_request_id()));
    let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: `fifo_path` is a valid NUL-terminated path for the duration of the call.
    assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);

    let report = run_pass(&mut fx.host(true), &fx.tick(Utc::now()));

    assert_eq!(fx.journal_len(), 0);
    assert_eq!(report.settled.len(), 3);
    assert!(report
        .settled
        .iter()
        .all(|(_, settle)| matches!(settle, Settle::Refused(_))));
    assert!(std::fs::read_dir(&inbox).unwrap().all(|name| !name
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".json")));
    assert!(outside.join("target.json").exists() && outside.join("hard.json").exists());
}

#[test]
fn a_verdict_needs_the_live_adjudicator() {
    let fx = fixture();
    fx.stage(StageStatus::NeedsAdjudication, None);
    fx.file_dispute(1);
    let judge = fx.record(SessionType::Adjudication, SessionStatus::Running);

    let dead = fx.relay(
        &judge,
        RequestKind::Verdict,
        payload_for(RequestKind::Verdict),
    );
    run_pass(&mut fx.host(false), &fx.tick(Utc::now()));
    assert_eq!(
        fx.outcome(&judge.id, &dead.id),
        Some(LedgerOutcome::Refused)
    );
    assert!(!verdict_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());

    let live = fx.relay(
        &judge,
        RequestKind::Verdict,
        payload_for(RequestKind::Verdict),
    );
    run_pass(&mut fx.host(true), &fx.tick(Utc::now()));
    assert_eq!(
        fx.outcome(&judge.id, &live.id),
        Some(LedgerOutcome::Applied)
    );
    assert!(verdict_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());

    // Guard 4 still holds on the relayed path: a recorded verdict stands.
    let again = fx.relay(
        &judge,
        RequestKind::Verdict,
        payload_for(RequestKind::Verdict),
    );
    run_pass(&mut fx.host(true), &fx.tick(Utc::now()));
    assert_eq!(
        fx.outcome(&judge.id, &again.id),
        Some(LedgerOutcome::Refused)
    );
}

#[test]
fn an_inbox_without_a_session_record_is_removed_after_a_day() {
    let fx = fixture();
    let ghost = fx.record(SessionType::Stage, SessionStatus::Running);
    let entry = entry_for(&ghost, RequestKind::Memory, memory_payload("orphaned"));
    std::fs::remove_file(
        fx.work_dir
            .join("sessions")
            .join(format!("{}.md", ghost.id)),
    )
    .unwrap();
    fx.plant(&ghost.id, &format!("{}.json", entry.id), &entry.encode());
    let mut host = fx.host(true);

    run_pass(&mut host, &fx.tick(Utc::now()));
    assert!(
        fx.inbox(&ghost.id).exists(),
        "left alone while its session may still appear"
    );

    run_pass(&mut host, &fx.tick(Utc::now() + Duration::hours(25)));
    assert!(!fx.inbox(&ghost.id).exists());
    assert_eq!(fx.journal_len(), 0);
}
