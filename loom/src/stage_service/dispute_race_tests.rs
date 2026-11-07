use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{CriterionField, DisputeKind};
use crate::models::stage::{dispute_budgets, Stage, StageStatus};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::contracts::store::{
    write_freeze, FreezeRecord, FrozenContract, FREEZE_RECORD_VERSION,
};
use crate::verify::transitions::{load_stage, save_stage, update_stage};
use anyhow::Result;
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::mpsc;

const CONTRACT: &str = "rejects-x";

thread_local! {
    static HOOK: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
    static ADMITTED_HOOK: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
    static PUBLISHED_HOOK: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
}

pub(super) fn before_stage_update() {
    HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook();
        }
    });
}

pub(super) fn after_admission() {
    ADMITTED_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook();
        }
    });
}

pub(super) fn after_publication() {
    PUBLISHED_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook();
        }
    });
}

/// Run `hook` on this thread just before its next filing takes the stage lock.
pub(super) fn on_before_stage_update(hook: Box<dyn FnOnce()>) {
    HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
}

/// Run `hook` on this thread once its next filing has written its request and
/// not yet its stage.
pub(super) fn on_published(hook: Box<dyn FnOnce()>) {
    PUBLISHED_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
}

pub(super) fn setup(id: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let wd = WorkDir::new(temp.path()).unwrap();
    wd.initialize().unwrap();
    let work = wd.root().to_path_buf();
    let mut stage = Stage {
        id: id.into(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    stage
        .acceptance
        .push(AcceptanceCriterion::Simple("true".into()));
    save_stage(&stage, &work).unwrap();
    (temp, work)
}

fn file(work: &Path, id: &str) -> Response {
    try_file(work, id).unwrap()
}

fn try_file(work: &Path, id: &str) -> Result<Response> {
    dispute::handle_dispute_criteria_identified(
        work,
        id,
        (CriterionField::Acceptance, vec![0]),
        "reason".into(),
        None,
        None,
        Some("race-receipt"),
    )
}

/// A plan v2 stage with a frozen contract, the fewest facts a kind admits.
pub(super) fn setup_contract(id: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let (temp, work) = setup(id);
    update_stage(id, &work, |stage| {
        stage.plan_version = 2;
        Ok(())
    })
    .unwrap();
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: id.into(),
        session_id: "session".into(),
        frozen_at: chrono::Utc::now(),
        base: "a".repeat(40),
        files: Vec::new(),
        contracts: vec![FrozenContract {
            id: CONTRACT.into(),
            adapter: None,
            outcome: "failed".into(),
            exit_code: Some(1),
            file: None,
            cwd: None,
            test: None,
        }],
    };
    write_freeze(&work, &record, &[]).unwrap();
    (temp, work)
}

fn contract_kind() -> DisputeKind {
    DisputeKind::Contract {
        contract_id: CONTRACT.into(),
    }
}

pub(super) fn try_file_contract(work: &Path, id: &str) -> Result<Response> {
    dispute_kinds::handle_file_dispute_identified(
        work,
        id,
        contract_kind(),
        "reason".into(),
        None,
        Some("race-receipt"),
    )
}

/// Nothing of this filing is left: no request, receipt or spent budget.
fn assert_nothing_filed(work: &Path, id: &str, status: StageStatus) {
    let stage = load_stage(id, work).unwrap();
    assert_eq!(stage.status, status);
    assert_eq!(stage.dispute_count, 0);
    assert_eq!(dispute_budgets::filed(&stage, &contract_kind()), 0);
    assert!(stage.tally.request_receipts.is_empty());
    let disputes = work.join("disputes").join(id);
    assert!(!disputes.join("8").exists());
    assert!(!disputes.join("1/request.md").exists());
    assert!(!disputes.join("requests/race-receipt.json").exists());
}

#[test]
fn admitted_filing_holds_stage_lock_until_publication() {
    let (_temp, work) = setup("admitted-stage");
    let (ready_tx, ready_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let thread_work = work.clone();
    let filing = std::thread::spawn(move || {
        ADMITTED_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                ready_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            }))
        });
        file(&thread_work, "admitted-stage")
    });
    ready_rx.recv().unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let thread_work = work.clone();
    let writer = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        update_stage("admitted-stage", &thread_work, |stage| {
            stage.status = StageStatus::Blocked;
            Ok(())
        })
        .unwrap();
        done_tx.send(()).unwrap();
    });
    started_rx.recv().unwrap();
    assert!(done_rx
        .recv_timeout(std::time::Duration::from_millis(100))
        .is_err());
    resume_tx.send(()).unwrap();
    assert!(matches!(
        filing.join().unwrap(),
        Response::DisputeCreated { .. }
    ));
    writer.join().unwrap();
    assert!(work.join("disputes/admitted-stage/1/request.md").exists());
    assert_eq!(
        load_stage("admitted-stage", &work).unwrap().dispute_count,
        1
    );
}

#[test]
fn status_change_before_admission_does_not_publish_or_spend() {
    let (_temp, work) = setup("race-stage");
    let (ready_tx, ready_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let thread_work = work.clone();
    let filing = std::thread::spawn(move || {
        HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                ready_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            }))
        });
        file(&thread_work, "race-stage")
    });
    ready_rx.recv().unwrap();
    update_stage("race-stage", &work, |stage| {
        stage.status = StageStatus::Blocked;
        Ok(())
    })
    .unwrap();
    resume_tx.send(()).unwrap();
    assert!(matches!(filing.join().unwrap(), Response::Error { .. }));
    let stage = load_stage("race-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.dispute_count, 0);
    assert!(stage.tally.request_receipts.is_empty());
    assert!(!work.join("disputes/race-stage/1/request.md").exists());
}

#[test]
fn status_change_before_admission_files_no_kind() {
    let (_temp, work) = setup_contract("race-kind");
    let (ready_tx, ready_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let thread_work = work.clone();
    let filing = std::thread::spawn(move || {
        HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                ready_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            }))
        });
        try_file_contract(&thread_work, "race-kind").unwrap()
    });
    ready_rx.recv().unwrap();
    update_stage("race-kind", &work, |stage| {
        stage.status = StageStatus::Blocked;
        Ok(())
    })
    .unwrap();
    resume_tx.send(()).unwrap();
    assert!(matches!(filing.join().unwrap(), Response::Error { .. }));
    assert_nothing_filed(&work, "race-kind", StageStatus::Blocked);
}

#[test]
fn an_admitted_kind_holds_the_stage_lock_until_publication() {
    let (_temp, work) = setup_contract("admitted-kind");
    let (ready_tx, ready_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let thread_work = work.clone();
    let filing = std::thread::spawn(move || {
        ADMITTED_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                ready_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            }))
        });
        try_file_contract(&thread_work, "admitted-kind").unwrap()
    });
    ready_rx.recv().unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let thread_work = work.clone();
    let writer = std::thread::spawn(move || {
        update_stage("admitted-kind", &thread_work, |stage| {
            stage.status = StageStatus::Blocked;
            Ok(())
        })
        .unwrap();
        done_tx.send(()).unwrap();
    });
    assert!(done_rx
        .recv_timeout(std::time::Duration::from_millis(100))
        .is_err());
    resume_tx.send(()).unwrap();
    assert!(matches!(
        filing.join().unwrap(),
        Response::DisputeCreated { id: 1 }
    ));
    writer.join().unwrap();
    let stage = load_stage("admitted-kind", &work).unwrap();
    assert_eq!(dispute_budgets::filed(&stage, &contract_kind()), 1);
    assert!(work.join("disputes/admitted-kind/1/request.md").exists());
}

/// A stage write failing after the request was written withdraws that request
/// and no other, spends nothing, and leaves the filing retryable.
fn failed_stage_write_withdraws_only_its_request(
    id: &str,
    work: &Path,
    filing: fn(&Path, &str) -> Result<Response>,
) {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let other = work.join("disputes").join(id).join("7");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("request.md"), "another open dispute").unwrap();
    let stages = work.join("stages");
    let denied = stages.clone();
    PUBLISHED_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            std::fs::set_permissions(denied, std::fs::Permissions::from_mode(0o555)).unwrap();
        }))
    });
    let failed = filing(work, id);
    std::fs::set_permissions(&stages, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(failed.is_err(), "{failed:?}");
    assert_nothing_filed(work, id, StageStatus::Executing);
    assert_eq!(
        std::fs::read_to_string(other.join("request.md")).unwrap(),
        "another open dispute"
    );
    assert!(matches!(
        filing(work, id).unwrap(),
        Response::DisputeCreated { id: 8 }
    ));
    assert!(work.join("disputes").join(id).join("8/request.md").exists());
}

#[test]
fn a_failed_stage_write_withdraws_a_criterion_request() {
    let (_temp, work) = setup("failed-criterion");
    failed_stage_write_withdraws_only_its_request("failed-criterion", &work, try_file);
}

#[test]
fn a_failed_stage_write_withdraws_a_kind_request() {
    let (_temp, work) = setup_contract("failed-kind");
    failed_stage_write_withdraws_only_its_request("failed-kind", &work, try_file_contract);
}
