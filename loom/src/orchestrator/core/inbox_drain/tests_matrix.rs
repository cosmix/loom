//! Section 5 of the plan, cell by cell, driven through the drain: each
//! session kind relays each request kind, and the observed outcome — with the
//! handler's effect checked — must equal the table.

use chrono::Utc;

use crate::models::dispute::verdict_file;
use crate::models::session::{SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::relay::RequestKind;
use crate::verify::transitions::load_stage;

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
/// merge-resolved, verdict, telemetry.
const SECTION_5: [(SessionType, [Cell; 7]); 5] = [
    (SessionType::Stage, [A, A, A, A, R, R, A]),
    (SessionType::Knowledge, [A, A, A, A, R, R, A]),
    (SessionType::Merge, [A, R, R, D, A, R, A]),
    (SessionType::Adjudication, [R, R, R, R, R, A, A]),
    (SessionType::BaseConflict, [A, R, R, D, R, R, A]),
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
        _ => fx.stage(StageStatus::Executing, Some(&record.id)),
    }
    let entry = fx.relay(&record, kind, payload_for(kind));
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
            assert_no_effect(&fx, &host);
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
        RequestKind::Dispute => assert_eq!(stage.status, StageStatus::NeedsAdjudication),
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
    }
    Cell::Apply
}

fn assert_no_effect(fx: &Fixture, host: &FakeHost) {
    assert_eq!(fx.journal_len(), 0);
    assert!(host.merges.is_empty());
    assert!(!fx.handoff_written());
    assert!(!verdict_file(&fx.work_dir.join("disputes"), STAGE, 1).exists());
    assert!(crate::telemetry::read_events(&fx.work_dir)
        .unwrap()
        .is_empty());
}
