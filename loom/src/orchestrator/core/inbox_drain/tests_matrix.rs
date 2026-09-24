//! Section 5 of the plan, with DESIGN D8's contract row and freeze column and
//! D15's file-dispute column, cell by cell, driven through the drain: each
//! session kind relays each
//! request kind, and the observed outcome — with the handler's effect
//! checked — must equal the table.

use chrono::Utc;

use crate::fs::stage_request::StageRequest;
use crate::models::dispute::{request_file, verdict_file};
use crate::models::session::{SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::relay::RequestKind;
use crate::verify::contracts::store::load_freeze;
use crate::verify::contracts::test_support::{contract_stage, contract_worktree, red_reports};
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
    (SessionType::Merge, [A, R, R, D, A, R, A, R, R]),
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
    let payload = match kind {
        RequestKind::FreezeContracts => serde_json::to_value(StageRequest::FreezeContracts {
            reports: red_reports(),
        })
        .unwrap(),
        _ => payload_for(kind),
    };
    let entry = fx.relay(&record, kind, payload);
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
        Settle::Applied(_) => classify_applied(&fx, &host, kind),
    }
}

/// Which applied cell this is, after confirming the handler's effect landed.
fn classify_applied(fx: &Fixture, host: &FakeHost, kind: RequestKind) -> Cell {
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    match kind {
        RequestKind::Memory => assert_eq!(fx.journal_len(), 1),
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
        RequestKind::Telemetry => {
            assert_eq!(
                crate::telemetry::read_events(&fx.work_dir).unwrap().len(),
                1
            );
        }
        RequestKind::FreezeContracts => {
            assert!(load_freeze(&fx.work_dir, STAGE).unwrap().is_some());
        }
    }
    Cell::Apply
}

fn assert_no_effect(fx: &Fixture, host: &FakeHost, kind: RequestKind) {
    assert_eq!(fx.journal_len(), 0);
    assert!(host.merges.is_empty());
    assert!(!fx.handoff_written());
    assert!(!verdict_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());
    assert!(crate::telemetry::read_events(&fx.work_dir)
        .unwrap()
        .is_empty());
    if kind == RequestKind::FileDispute {
        assert!(!request_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());
    } else {
        assert!(load_freeze(&fx.work_dir, STAGE).unwrap().is_none());
    }
}
