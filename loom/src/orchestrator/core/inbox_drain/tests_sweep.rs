//! Retirement and stale tickets.

use std::path::PathBuf;

use chrono::{Duration, Utc};

use crate::fs::inbox::LedgerOutcome;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::orchestrator::terminal::native::session_settings_path;
use crate::relay::{new_request_id, RequestKind, Ticket};

use super::sweep::sweep_sessions;
use super::test_support::{fixture, memory_payload, payload_for, Fixture};
use super::PassReport;

fn scratch_dir(fx: &Fixture, record: &Session) -> PathBuf {
    let dir = fx.scratch_root.join(&record.id);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_ticket(dir: &std::path::Path, kind: RequestKind) -> PathBuf {
    let ticket = Ticket {
        v: 1,
        id: new_request_id(),
        kind,
        created_at: Utc::now(),
        payload: payload_for(kind),
    };
    let path = dir.join(format!("{}.req", ticket.id));
    std::fs::write(&path, ticket.encode()).unwrap();
    path
}

fn write_capsule(fx: &Fixture, record: &Session) -> PathBuf {
    let capsule = session_settings_path(&fx.work_dir, &record.id);
    std::fs::create_dir_all(capsule.parent().unwrap()).unwrap();
    std::fs::write(&capsule, "{}").unwrap();
    capsule
}

#[test]
fn retirement_drains_once_more_then_removes_everything_but_the_ledger() {
    let fx = fixture();
    fx.stage(StageStatus::Completed, None);
    let record = fx.record(SessionType::Knowledge, SessionStatus::Completed);
    let note = fx.relay(&record, RequestKind::Memory, memory_payload("last words"));
    let scratch = scratch_dir(&fx, &record);
    write_ticket(&scratch, RequestKind::Memory);
    std::fs::write(fx.inbox(&record.id).join(".tmp").join("partial"), "x").unwrap();
    let capsule = write_capsule(&fx, &record);
    let mut report = PassReport::default();

    sweep_sessions(&mut fx.host(false), &fx.tick(Utc::now()), &mut report);

    assert_eq!(report.retired, vec![record.id.clone()]);
    assert_eq!(
        fx.journal_len(),
        1,
        "the final drain applied the pending note"
    );
    assert_eq!(
        fx.outcome(&record.id, &note.id),
        Some(LedgerOutcome::Applied)
    );
    assert!(!scratch.exists());
    assert!(!capsule.exists());
    assert!(!fx.inbox(&record.id).join(".tmp").exists());
    assert!(fx.inbox(&record.id).join("ledger.jsonl").exists());
}

#[test]
fn a_session_whose_only_leftover_is_its_settings_capsule_is_retired() {
    let fx = fixture();
    let record = fx.record(SessionType::Stage, SessionStatus::Completed);
    let capsule = write_capsule(&fx, &record);
    let mut report = PassReport::default();

    sweep_sessions(&mut fx.host(false), &fx.tick(Utc::now()), &mut report);

    assert_eq!(report.retired, vec![record.id.clone()]);
    assert!(!capsule.exists());
}

#[test]
fn a_finished_session_whose_process_still_lives_is_not_retired() {
    let fx = fixture();
    let record = fx.record(SessionType::Stage, SessionStatus::Completed);
    let scratch = scratch_dir(&fx, &record);
    let capsule = write_capsule(&fx, &record);
    let mut report = PassReport::default();

    sweep_sessions(&mut fx.host(true), &fx.tick(Utc::now()), &mut report);

    assert!(report.retired.is_empty());
    assert!(scratch.exists() && capsule.exists());
}

#[test]
fn a_running_session_is_never_retired() {
    let fx = fixture();
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    let scratch = scratch_dir(&fx, &record);
    let mut report = PassReport::default();

    sweep_sessions(&mut fx.host(false), &fx.tick(Utc::now()), &mut report);

    assert!(report.retired.is_empty());
    assert!(scratch.exists());
}

#[test]
fn the_stale_ticket_warning_fires_once_per_session() {
    let fx = fixture();
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    let scratch = scratch_dir(&fx, &record);
    let ticket = write_ticket(&scratch, RequestKind::Block);
    let mut host = fx.host(true);

    let mut fresh = PassReport::default();
    sweep_sessions(&mut host, &fx.tick(Utc::now()), &mut fresh);
    assert!(
        fresh.stalled_warned.is_empty(),
        "a ticket under a minute old is not stalled"
    );

    let later = Utc::now() + Duration::minutes(2);
    let mut first = PassReport::default();
    sweep_sessions(&mut host, &fx.tick(later), &mut first);
    let mut second = PassReport::default();
    sweep_sessions(&mut host, &fx.tick(later), &mut second);

    assert_eq!(first.stalled_warned, vec![record.id.clone()]);
    assert!(second.stalled_warned.is_empty());
    assert!(
        ticket.exists(),
        "a stalled control ticket is reported, never deleted"
    );
}

#[test]
fn old_telemetry_tickets_are_deleted_silently() {
    let fx = fixture();
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    let scratch = scratch_dir(&fx, &record);
    let ticket = write_ticket(&scratch, RequestKind::Telemetry);
    let mut host = fx.host(true);

    let mut young = PassReport::default();
    sweep_sessions(
        &mut host,
        &fx.tick(Utc::now() + Duration::minutes(2)),
        &mut young,
    );
    assert!(ticket.exists());

    let mut old = PassReport::default();
    sweep_sessions(
        &mut host,
        &fx.tick(Utc::now() + Duration::minutes(11)),
        &mut old,
    );

    assert!(!ticket.exists());
    assert!(young.stalled_warned.is_empty() && old.stalled_warned.is_empty());
}
