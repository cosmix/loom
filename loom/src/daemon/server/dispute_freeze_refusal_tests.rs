//! Loom's own freeze-refusal dispute (`file_freeze_refusal`) against a real
//! state directory: who it is filed for, on which stage, its budget and its
//! receipt, the escalation once the budget is spent, and the transports that
//! refuse the kind from anyone else.

use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::contracts::test_support::{contract, write_test_freeze};
use crate::verify::transitions::save_stage;

pub(super) const FREEZE_STAGE: &str = "stage-freeze";
const KEY: &str = "freeze-refusal-session-1-2026-10-23T10:00:00.123+00:00";
const PROBLEM: &str = "contract 'rejects-x': tests/x_contract.rs is not compiled";

/// A filing precondition: its label and the change to the state directory
/// that breaks it.
type Case = (&'static str, fn(&Path));

/// A state directory holding [`FREEZE_STAGE`] in its open contract phase: an
/// executing v2 standard stage with one contract and no freeze, owned by a
/// running session of type `owner`.
pub(super) fn contract_phase(owner: SessionType) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let wd = WorkDir::new(tmp.path()).unwrap();
    wd.initialize().unwrap();
    let work_dir = wd.root().to_path_buf();
    let mut session = Session::new();
    session.session_type = owner;
    session.assign_to_stage(FREEZE_STAGE.to_string());
    session.status = SessionStatus::Running;
    save_session(&session, &work_dir).unwrap();
    let stage = Stage {
        id: FREEZE_STAGE.to_string(),
        name: "Freeze".to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        session: Some(session.id),
        contracts: vec![contract()],
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    (tmp, work_dir)
}

pub(super) fn freeze_refusal() -> DisputeKind {
    DisputeKind::FreezeRefusal {
        problems: vec![PROBLEM.to_string()],
    }
}

fn file(work_dir: &Path, key: &str) -> FreezeRefusalFiling {
    file_freeze_refusal(work_dir, FREEZE_STAGE, vec![PROBLEM.to_string()], key).unwrap()
}

pub(super) fn request_path(work_dir: &Path, id: u32) -> PathBuf {
    work_dir.join(format!("disputes/{FREEZE_STAGE}/{id}/request.md"))
}

fn declined(filing: FreezeRefusalFiling) -> String {
    match filing {
        FreezeRefusalFiling::Declined(reason) => reason,
        other => panic!("expected a declined filing, got {other:?}"),
    }
}

fn set(work_dir: &Path, change: impl FnOnce(&mut Stage)) {
    update_stage(FREEZE_STAGE, work_dir, |stage| {
        change(stage);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_refused_freeze_is_filed_for_the_contract_writer() {
    let (_tmp, work_dir) = contract_phase(SessionType::Contract);

    let filing = file_freeze_refusal(
        &work_dir,
        FREEZE_STAGE,
        vec![format!("writer-reported: {PROBLEM}\n\nsecond\x1b[2J line")],
        KEY,
    )
    .unwrap();

    assert!(
        matches!(filing, FreezeRefusalFiling::Filed(_)),
        "{filing:?}"
    );
    let content = std::fs::read_to_string(request_path(&work_dir, 1)).unwrap();
    let request: crate::models::dispute::DisputeRequest =
        crate::yaml::from_str(content.split("---").nth(1).unwrap()).unwrap();
    let DisputeKind::FreezeRefusal { problems } = request.kind else {
        panic!("expected a freeze-refusal request, got {:?}", request.kind);
    };
    assert_eq!(problems[0], format!("writer-reported: {PROBLEM}"));
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(!problems[1].contains('\x1b'), "{problems:?}");
    let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert_eq!(stage.tally.freeze_refusal_disputes, 1);
    let reason = stage.review_reason.unwrap_or_default();
    assert!(
        reason.starts_with("contract freeze refused: writer-reported:"),
        "{reason}"
    );
}

#[test]
fn a_freeze_refusal_needs_an_open_contract_phase_and_its_writer() {
    let cases: [Case; 5] = [
        ("already frozen", |w| {
            write_test_freeze(w, FREEZE_STAGE, "s")
        }),
        ("not executing", |w| {
            set(w, |s| s.status = StageStatus::Queued)
        }),
        ("plan version 1", |w| set(w, |s| s.plan_version = 1)),
        ("no contracts", |w| set(w, |s| s.contracts.clear())),
        ("owned by an implementer", |w| {
            let mut implementer = Session::new();
            implementer.assign_to_stage(FREEZE_STAGE.to_string());
            save_session(&implementer, w).unwrap();
            set(w, |s| s.session = Some(implementer.id));
        }),
    ];
    for (case, make) in cases {
        let (_tmp, work_dir) = contract_phase(SessionType::Contract);
        make(&work_dir);

        let reason = declined(file(&work_dir, KEY));

        assert!(!reason.is_empty(), "{case}");
        assert!(!request_path(&work_dir, 1).exists(), "{case}: {reason}");
        let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
        assert_eq!(stage.tally.freeze_refusal_disputes, 0, "{case}");
    }
}

#[test]
fn a_contract_writer_cannot_dispute_a_frozen_contract() {
    let (_tmp, work_dir) = contract_phase(SessionType::Contract);
    write_test_freeze(&work_dir, FREEZE_STAGE, "s");
    let kind = DisputeKind::Contract {
        contract_id: contract().id,
    };

    let response = handle_file_dispute(&work_dir, FREEZE_STAGE, kind, "r".into(), None).unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    assert!(message.contains("contract session"), "{message}");
    assert!(!request_path(&work_dir, 1).exists());
}

#[test]
fn a_criterion_dispute_is_still_admitted_for_a_knowledge_session() {
    let (_tmp, work_dir) = contract_phase(SessionType::Knowledge);
    let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
    let criterion = DisputeKind::criterion(Default::default(), vec![0]);

    check_filer(&work_dir, &stage, &criterion).unwrap();
    let refusal = check_filer(&work_dir, &stage, &freeze_refusal()).unwrap_err();
    assert!(
        format!("{refusal:#}").contains("knowledge session"),
        "{refusal:#}"
    );
}

#[test]
fn filings_one_to_three_are_admitted_and_the_fourth_is_not_escalated_here() {
    let (_tmp, work_dir) = contract_phase(SessionType::Contract);
    for filing in 1..=MAX_DISPUTES_PER_KIND {
        set(&work_dir, |s| s.status = StageStatus::Executing);
        let outcome = file(&work_dir, &format!("{KEY}-{filing}"));
        assert!(
            matches!(outcome, FreezeRefusalFiling::Filed(_)),
            "{outcome:?}"
        );
        let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
        assert_eq!(stage.tally.freeze_refusal_disputes, filing);
    }
    set(&work_dir, |s| s.status = StageStatus::Executing);

    let reason = declined(file(&work_dir, &format!("{KEY}-4")));

    assert!(reason.contains("budget"), "{reason}");
    assert!(!request_path(&work_dir, MAX_DISPUTES_PER_KIND + 1).exists());
    let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing, "the caller escalates");
}

#[test]
fn the_same_request_key_files_one_request() {
    let (_tmp, work_dir) = contract_phase(SessionType::Contract);

    let first = file(&work_dir, KEY);
    let again = file(&work_dir, KEY);

    assert_eq!(first, again);
    assert!(request_path(&work_dir, 1).exists());
    assert!(!request_path(&work_dir, 2).exists());
    let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
    assert_eq!(stage.tally.freeze_refusal_disputes, 1);
}

#[test]
fn the_transports_refuse_a_freeze_refusal_and_write_nothing() {
    let (_tmp, work_dir) = contract_phase(SessionType::Contract);

    let socket = handle_file_dispute(&work_dir, FREEZE_STAGE, freeze_refusal(), "r".into(), None);
    let spool = handle_file_dispute_identified(
        &work_dir,
        FREEZE_STAGE,
        freeze_refusal(),
        "r".into(),
        None,
        Some("spooled-1"),
    );

    for response in [socket.unwrap(), spool.unwrap()] {
        let Response::Error { message } = response else {
            panic!("expected a refusal, got {response:?}");
        };
        assert!(message.contains("filed by loom only"), "{message}");
    }
    assert!(!request_path(&work_dir, 1).exists());
    let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.tally.freeze_refusal_disputes, 0);
}

#[test]
fn a_spent_refusal_budget_escalates_with_the_last_problems() {
    let (_tmp, work_dir) = contract_phase(SessionType::Contract);

    escalate_freeze_refusal(
        &work_dir,
        FREEZE_STAGE,
        &[PROBLEM.to_string()],
        Some("session(s) s-9 survived the retirement kill"),
    )
    .unwrap();

    let stage = load_stage(FREEZE_STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap_or_default();
    assert!(
        reason.starts_with("contract freeze refused 4 times"),
        "{reason}"
    );
    assert!(reason.contains(PROBLEM), "{reason}");
    assert!(
        reason.contains(&format!("loom stage human-review {FREEZE_STAGE}")),
        "{reason}"
    );
    assert!(reason.contains("s-9 survived"), "{reason}");
}
