//! Section 5 of the plan, with DESIGN D8's contract row and freeze column and
//! D15's file-dispute column, cell by cell, driven through the drain: each
//! session kind relays each
//! request kind, and the observed outcome — with the handler's effect
//! checked — must equal the table.

use chrono::Utc;

use crate::fs::stage_request::StageRequest;
use crate::host_verify::store::load_current;
use crate::host_verify::{digest, JobKind};
use crate::models::dispute::{request_file, verdict_file};
use crate::models::session::{SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::relay::RequestKind;
use crate::verify::contracts::store::load_freeze;
use crate::verify::contracts::test_support::{contract_stage, contract_worktree};
use crate::verify::transitions::{load_stage, save_stage};

use super::test_support::{fixture, payload_for, FakeHost, Fixture, STAGE};
use super::{run_pass, Settle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Apply,
    DocumentOnly,
    Refuse,
}

use Cell::{Apply as A, DocumentOnly as D, Refuse as R};

/// In `RequestKind::all()` order: memory, block, dispute, handoff,
/// merge-resolved, verdict, telemetry, freeze-contracts, file-dispute.
const SECTION_5: [(SessionType, [Cell; 9]); 6] = [
    (SessionType::Stage, [A, A, A, A, R, R, A, R, A]),
    (SessionType::Knowledge, [A, A, A, A, R, R, A, R, R]),
    (SessionType::Merge, [A, A, R, D, A, R, A, R, R]),
    (SessionType::Adjudication, [R, R, R, R, R, A, A, R, R]),
    (SessionType::BaseConflict, [A, R, R, D, R, R, A, R, R]),
    (SessionType::Contract, [A, A, R, A, R, R, A, A, R]),
];

#[test]
fn the_drain_applies_exactly_the_section_5_matrix() {
    for (session_type, row) in SECTION_5 {
        for (kind, expected) in RequestKind::all().into_iter().zip(row) {
            assert_eq!(
                run_cell(session_type, kind),
                expected,
                "{session_type} session relaying '{kind}'"
            );
        }
    }
}

fn run_cell(session_type: SessionType, kind: RequestKind) -> Cell {
    let fx = fixture();
    let record = fx.record(session_type, SessionStatus::Running);
    match kind {
        RequestKind::Verdict => {
            fx.stage(StageStatus::NeedsAdjudication, Some(&record.id));
            fx.file_dispute(1);
        }
        RequestKind::MergeResolved => fx.stage(StageStatus::MergeConflict, Some(&record.id)),
        // A resolver gives up on a stage that is waiting for its merge.
        RequestKind::Block if session_type == SessionType::Merge => {
            fx.stage(StageStatus::MergeConflict, None)
        }
        RequestKind::FreezeContracts => {
            // Everything the freeze handler needs to succeed, so a refusal
            // can only come from the matrix.
            contract_worktree(&fx.repo_root, STAGE);
            save_stage(&contract_stage(STAGE, &record.id), &fx.work_dir).unwrap();
        }
        // A v2 stage with a frozen contract to dispute, so a refusal can only
        // come from the matrix.
        RequestKind::FileDispute => fx.frozen_contract_stage(&record.id),
        _ => fx.stage(StageStatus::Executing, Some(&record.id)),
    }
    let entry = fx.relay(&record, kind, cell_payload(&fx, kind));
    let mut host = fx.host(true);

    let report = run_pass(&mut host, &fx.tick(Utc::now()));

    let settled = report
        .settled
        .iter()
        .find(|(id, _)| *id == entry.id)
        .map(|(_, settle)| settle.clone())
        .expect("every relayed entry is settled in one pass");
    match settled {
        Settle::Refused(_) => {
            assert_no_effect(&fx, &host, kind);
            Cell::Refuse
        }
        Settle::Applied(_) => classify_applied(&fx, &host, session_type, kind),
    }
}

/// The request body a cell relays. A freeze carries the worktree's own HEAD and
/// digest, as the freeze command sends.
fn cell_payload(fx: &Fixture, kind: RequestKind) -> serde_json::Value {
    match kind {
        RequestKind::FreezeContracts => {
            let worktree = fx.repo_root.join(".worktrees").join(STAGE);
            serde_json::to_value(StageRequest::FreezeContracts {
                head: digest::head(&worktree).unwrap(),
                tree_digest: digest::tree_digest(&worktree).unwrap(),
            })
            .unwrap()
        }
        _ => payload_for(kind),
    }
}

/// Which applied cell this is, after confirming the handler's effect landed.
fn classify_applied(
    fx: &Fixture,
    host: &FakeHost,
    session_type: SessionType,
    kind: RequestKind,
) -> Cell {
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    match kind {
        RequestKind::Memory => assert_eq!(fx.journal_len(), 1),
        // A resolver's block is a give-up the host carries out; any other
        // session's block is the stage block itself.
        RequestKind::Block if session_type == SessionType::Merge => {
            assert_eq!(host.give_ups.len(), 1);
            assert_eq!(stage.status, StageStatus::MergeConflict);
        }
        RequestKind::Block => assert_eq!(stage.status, StageStatus::Blocked),
        RequestKind::Dispute | RequestKind::FileDispute => {
            assert_eq!(stage.status, StageStatus::NeedsAdjudication)
        }
        RequestKind::Handoff => {
            assert!(fx.handoff_written());
            if stage.status == StageStatus::Executing {
                return Cell::DocumentOnly;
            }
            assert_eq!(stage.status, StageStatus::NeedsHandoff);
        }
        RequestKind::MergeResolved => assert_eq!(host.merges.len(), 1),
        RequestKind::Verdict => {
            assert!(verdict_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());
        }
        RequestKind::Telemetry => assert_telemetry_recorded(fx),
        // The freeze is a contract run on the host: the drain queues it as
        // the stage's current host job, and the daemon freezes when it
        // applies the run's result.
        RequestKind::FreezeContracts => assert_freeze_queued(fx),
        RequestKind::Commit => {
            unreachable!("SECTION_5 has no commit column; commit_tests covers the commit kind")
        }
        RequestKind::Check => {
            unreachable!("SECTION_5 has no check column; check.rs tests cover the check kind")
        }
        RequestKind::Complete => {
            unreachable!("SECTION_5 has no complete column; complete.rs tests cover the kind")
        }
    }
    Cell::Apply
}

fn assert_telemetry_recorded(fx: &Fixture) {
    assert_eq!(
        crate::telemetry::read_events(&fx.work_dir).unwrap().len(),
        1
    );
}

fn assert_freeze_queued(fx: &Fixture) {
    let job = load_current(&fx.work_dir, STAGE).unwrap();
    assert_eq!(job.map(|job| job.kind), Some(JobKind::Freeze));
}

fn assert_no_effect(fx: &Fixture, host: &FakeHost, kind: RequestKind) {
    assert_eq!(fx.journal_len(), 0);
    assert!(host.merges.is_empty());
    assert!(host.give_ups.is_empty());
    assert!(!fx.handoff_written());
    assert!(!verdict_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());
    assert!(crate::telemetry::read_events(&fx.work_dir)
        .unwrap()
        .is_empty());
    assert!(load_current(&fx.work_dir, STAGE).unwrap().is_none());
    if kind == RequestKind::FileDispute {
        assert!(!request_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());
    } else {
        assert!(load_freeze(&fx.work_dir, STAGE).unwrap().is_none());
    }
}
