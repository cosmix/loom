//! A filing against the session lock order, and a crash between its two writes.

use super::dispute_race_tests::{
    on_before_stage_update, on_published, setup, setup_contract, try_file_contract,
};
use super::{dispute, dispute_store, receipts, Response};
use crate::fs::locking::locked_dir_update;
use crate::fs::session_files::save_session;
use crate::models::dispute::{
    verdict_file, CriterionField, DisputeKind, DisputeRequest, DisputeVerdict, DisputeVerdictRecord,
};
use crate::models::session::{Session, SessionType};
use crate::models::stage::StageStatus;
use crate::orchestrator::adjudication::{attempt_count, AdjudicatorRegistry};
use crate::verify::transitions::{load_stage, update_stage};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

const BOUND: Duration = Duration::from_secs(10);

/// `complete_stage` holds the sessions-directory lock across `update_stage`,
/// so a filing that holds the stage lock must not wait on the sessions lock.
#[test]
fn a_filing_does_not_deadlock_a_sessions_lock_holder_waiting_on_stages() {
    let (_temp, work) = setup_contract("held");
    let mut session = Session::new();
    session.session_type = SessionType::Stage;
    session.stage_id = Some("held".into());
    save_session(&session, &work).unwrap();
    update_stage("held", &work, |stage| {
        stage.session = Some(session.id.clone());
        Ok(())
    })
    .unwrap();

    let (holding_tx, holding_rx) = mpsc::channel();
    let (filing_tx, filing_rx) = mpsc::channel();
    let (held_tx, held_rx) = mpsc::channel();
    let holder_work = work.clone();
    std::thread::spawn(move || {
        let held = locked_dir_update(&holder_work.join("sessions"), || {
            holding_tx.send(()).unwrap();
            filing_rx.recv().unwrap();
            // The filing has the stage lock by now, if this order is to matter.
            std::thread::sleep(Duration::from_millis(200));
            update_stage("held", &holder_work, |_| Ok(()))
        });
        held_tx.send(held.map(|_| ())).unwrap();
    });
    holding_rx.recv().unwrap();

    let (filed_tx, filed_rx) = mpsc::channel();
    let filing_work = work.clone();
    std::thread::spawn(move || {
        on_before_stage_update(Box::new(move || filing_tx.send(()).unwrap()));
        filed_tx
            .send(try_file_contract(&filing_work, "held"))
            .unwrap();
    });

    let filed = filed_rx
        .recv_timeout(BOUND)
        .expect("the filing deadlocked against the sessions lock holder");
    assert!(matches!(filed.unwrap(), Response::DisputeCreated { id: 1 }));
    held_rx
        .recv_timeout(BOUND)
        .expect("the sessions lock holder deadlocked against the filing")
        .unwrap();
}

/// What a crash between the two writes leaves: a request on disk, the stage
/// unchanged.
fn crash_before_stage_write(work: &Path, stage_id: &str) -> u32 {
    let stage = load_stage(stage_id, work).unwrap();
    let locked = dispute_store::lock_stage_disputes(work, stage_id).unwrap();
    dispute_store::write_request(&locked.stage_dir, criterion_record(&stage)).unwrap()
}

fn criterion_record(stage: &crate::models::stage::Stage) -> DisputeRequest {
    dispute::build_record(
        stage,
        DisputeKind::criterion(CriterionField::Acceptance, vec![0]),
        "reason".into(),
        None,
        None,
    )
}

fn file_keyless(work: &Path, stage_id: &str) -> Response {
    dispute::handle_dispute_criteria(
        work,
        stage_id,
        CriterionField::Acceptance,
        vec![0],
        "reason".into(),
        None,
        None,
    )
    .unwrap()
}

fn offered(work: &Path) -> Vec<u32> {
    AdjudicatorRegistry::new()
        .disputes_awaiting_session(work)
        .unwrap()
        .jobs
        .iter()
        .map(|job| job.request.id)
        .collect()
}

fn write_evidence_verdict(work: &Path, stage_id: &str, id: u32) {
    let record = DisputeVerdictRecord {
        id,
        stage_id: stage_id.to_string(),
        verdict: DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".to_string()],
        },
        adjudicator_attempt_count: 1,
        created_at: chrono::Utc::now(),
        model: "test".to_string(),
        session_id: None,
    };
    let yaml = crate::yaml::to_string(&record).unwrap();
    let path = verdict_file(&work.join("disputes"), stage_id, id);
    std::fs::write(path, format!("---\n{yaml}---\n\n# Verdict\n")).unwrap();
}

/// The refiled dispute is the one judged, for the one budget unit it spent;
/// the request the crash orphaned is settled, so it holds nothing up.
#[test]
fn a_request_orphaned_by_a_crash_is_not_adjudicated_beside_its_refiling() {
    let (_temp, work) = setup("crashed");
    assert_eq!(crash_before_stage_write(&work, "crashed"), 1);
    assert_eq!(
        file_keyless(&work, "crashed"),
        Response::DisputeCreated { id: 2 }
    );
    assert_eq!(load_stage("crashed", &work).unwrap().dispute_count, 1);

    assert_eq!(offered(&work), vec![2]);

    assert_eq!(attempt_count(&work, "crashed", 1), 0);
    assert!(work.join("disputes/crashed/1/closed.marker").exists());
    write_evidence_verdict(&work, "crashed", 2);
    AdjudicatorRegistry::new()
        .apply_pending_verdicts(&work)
        .unwrap();
    assert_eq!(
        load_stage("crashed", &work).unwrap().status,
        StageStatus::Queued,
        "the orphaned request must not hold the stage in adjudication",
    );
}

/// A pass that reads a request its filing has not yet named in the stage waits
/// for that filing instead of settling a live dispute.
#[test]
fn a_pass_does_not_settle_the_request_of_a_filing_in_flight() {
    let (_temp, work) = setup("in-flight");
    assert_eq!(
        file_keyless(&work, "in-flight"),
        Response::DisputeCreated { id: 1 }
    );
    std::fs::write(work.join("disputes/in-flight/1/closed.marker"), "").unwrap();

    let (ready_tx, ready_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel::<()>();
    let filing_work = work.clone();
    let filing = std::thread::spawn(move || {
        on_published(Box::new(move || {
            ready_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
        }));
        file_keyless(&filing_work, "in-flight")
    });
    ready_rx.recv().unwrap();

    let (passed_tx, passed_rx) = mpsc::channel();
    let pass_work = work.clone();
    std::thread::spawn(move || passed_tx.send(offered(&pass_work)).unwrap());
    assert!(passed_rx.recv_timeout(Duration::from_millis(200)).is_err());

    resume_tx.send(()).unwrap();
    assert_eq!(filing.join().unwrap(), Response::DisputeCreated { id: 2 });
    passed_rx.recv_timeout(BOUND).unwrap();
    assert!(!work.join("disputes/in-flight/2/closed.marker").exists());
    assert_eq!(offered(&work), vec![2]);
}

/// A retried filing reuses the id its receipt reserved, unless that request
/// was settled meanwhile: a closed request would never be judged.
#[test]
fn a_reservation_of_a_closed_request_is_not_reused() {
    let (_temp, work) = setup("reserved");
    let stage = load_stage("reserved", &work).unwrap();
    let dir = work.join("disputes/reserved");
    std::fs::create_dir_all(&dir).unwrap();
    let file = |key| receipts::write_request(&dir, criterion_record(&stage), Some(key)).unwrap();
    assert_eq!(file("key"), 1);
    assert_eq!(file("key"), 1);
    std::fs::write(dir.join("1/closed.marker"), "").unwrap();
    assert_eq!(file("key"), 2);
    assert!(dir.join("2/request.md").exists());
}
