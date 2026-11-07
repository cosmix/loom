//! Which verdicts hold a stage for the daemon's completion run when siblings
//! are in play, and how `hand_back_held_stage` re-queues a held stage, each
//! driven from the files the daemon reads.

use serde_json::json;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::tests::{make_stage, reject_verdict, write_request, write_stage, write_verdict};
use super::{feedback, hand_back_held_stage, is_held_for_completion, AdjudicatorRegistry};
use crate::models::dispute::{Citation, DisputeKind, DisputeVerdict, FindingSnapshot, PlanPatch};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::impact_tests::{FailureKind, ImpactFailure};
use crate::verify::integrity::{EventKind, IntegrityEvent};
use crate::verify::transitions::load_stage;

fn work_dir() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("stages")).unwrap();
    tmp
}

fn apply(work: &Path, dispute: u32) {
    AdjudicatorRegistry::new()
        .apply_verdict(work, "s1", dispute)
        .unwrap();
}

fn accept() -> DisputeVerdict {
    DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: json!({}) },
        citations: vec![Citation {
            file: "src/a.rs".to_string(),
            line: Some(4),
            excerpt: "let x = y;".to_string(),
            claim: "the disputed change is sound".to_string(),
        }],
        reasoning: "the disputed change is sound".to_string(),
    }
}

fn integrity() -> DisputeKind {
    let event = IntegrityEvent {
        id: "TI-assert-rust".to_string(),
        kind: EventKind::AssertTotal,
        language: Some("rust".to_string()),
        path: None,
        base: Some(120),
        current: Some(118),
        current_sha256: None,
        detail: Vec::new(),
    };
    DisputeKind::Integrity {
        event_ids: vec![event.id.clone()],
        evidence: vec![event],
    }
}

fn impact() -> DisputeKind {
    let failure = ImpactFailure {
        id: "IT-pytest:deps/kit".to_string(),
        command: "uv run pytest tests/test_kit.py".to_string(),
        tests: vec!["tests/test_kit.py".to_string()],
        kind: FailureKind::DoesNotBuild,
        base: "the stage's base 0123abcd proved nothing".to_string(),
        accepted: None,
        output: "ModuleNotFoundError: No module named 'kit_native'".to_string(),
    };
    DisputeKind::ImpactTests {
        test_ids: vec![failure.id.clone()],
        evidence: vec![failure],
    }
}

/// `stage` saved with dispute 1, an integrity dispute, accepted and applied.
fn verdict_held(work: &Path, stage: &Stage) -> Stage {
    write_stage(work, stage);
    write_request(work, "s1", 1, integrity());
    write_verdict(work, "s1", 1, accept(), 1);
    apply(work, 1);
    load_stage("s1", work).unwrap()
}

/// Every stage file with its bytes, to prove a call wrote nothing.
fn stage_files(work: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(work.join("stages"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    files.sort();
    files
}

#[test]
fn an_accepted_impact_dispute_alone_holds_the_stage() {
    let tmp = work_dir();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    write_request(work, "s1", 1, impact());
    write_verdict(work, "s1", 1, accept(), 1);

    apply(work, 1);

    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert!(is_held_for_completion(work, &stage));
}

#[test]
fn an_open_sibling_defers_the_hold_to_the_last_verdict() {
    let tmp = work_dir();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    write_request(work, "s1", 1, integrity());
    write_request(work, "s1", 2, integrity());
    write_verdict(work, "s1", 1, accept(), 1);

    apply(work, 1);

    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert!(!is_held_for_completion(work, &stage));

    write_verdict(work, "s1", 2, accept(), 1);
    apply(work, 2);

    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert!(is_held_for_completion(work, &stage));
}

#[test]
fn a_last_verdict_that_requeues_overrides_an_earlier_hold() {
    let tmp = work_dir();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    write_request(work, "s1", 1, integrity());
    let contract = DisputeKind::Contract {
        contract_id: "rejects-x".to_string(),
    };
    write_request(work, "s1", 2, contract);
    write_verdict(work, "s1", 1, accept(), 1);
    apply(work, 1);
    write_verdict(work, "s1", 2, reject_verdict(), 1);

    apply(work, 2);

    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(!is_held_for_completion(work, &stage));
}

#[test]
fn a_findings_verdict_with_no_rulings_requeues() {
    let tmp = work_dir();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    let kind = DisputeKind::Findings {
        finding_ids: Vec::new(),
        evidence: Vec::<FindingSnapshot>::new(),
    };
    write_request(work, "s1", 1, kind);
    let rulings = DisputeVerdict::Rulings {
        rulings: Vec::new(),
    };
    write_verdict(work, "s1", 1, rulings, 1);

    apply(work, 1);

    assert_eq!(load_stage("s1", work).unwrap().status, StageStatus::Queued);
}

#[test]
fn hand_back_requeues_a_held_stage_and_lists_its_failures_after_the_notice() {
    let tmp = work_dir();
    let work = tmp.path();
    verdict_held(work, &make_stage("s1"));
    let notice = feedback::read_feedback(work, "s1").unwrap().unwrap();
    let failures = [
        "criterion 1 failed".to_string(),
        "wiring failed".to_string(),
    ];

    assert!(hand_back_held_stage(work, "s1", &failures).unwrap());

    assert_eq!(load_stage("s1", work).unwrap().status, StageStatus::Queued);
    let content = feedback::read_feedback(work, "s1").unwrap().unwrap();
    let section = content
        .strip_prefix(notice.as_str())
        .expect("the verdict notice stays first");
    assert!(
        section.contains("\n- criterion 1 failed\n- wiring failed\n"),
        "{section}"
    );
}

#[test]
fn hand_back_changes_nothing_for_a_stage_that_is_not_verdict_held() {
    let open = make_stage("s1");
    let with_session = Stage {
        session: Some("session-1".to_string()),
        ..make_stage("s1")
    };
    let queued = Stage {
        status: StageStatus::Queued,
        ..make_stage("s1")
    };
    for (stage, open_dispute) in [(open, true), (with_session, false), (queued, false)] {
        let tmp = work_dir();
        let work = tmp.path();
        write_stage(work, &stage);
        if open_dispute {
            write_request(work, "s1", 1, integrity());
        }
        let before = stage_files(work);

        let moved = hand_back_held_stage(work, "s1", &["failed".to_string()]).unwrap();

        assert!(!moved, "{:?}", stage.status);
        assert_eq!(stage_files(work), before, "{:?}", stage.status);
        assert!(feedback::read_feedback(work, "s1").unwrap().is_none());
    }
}

#[test]
fn an_operator_hold_blocks_completion_but_not_the_hand_back() {
    let tmp = work_dir();
    let work = tmp.path();
    let stage = verdict_held(
        work,
        &Stage {
            held: true,
            ..make_stage("s1")
        },
    );
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert!(!is_held_for_completion(work, &stage));
    let notice = feedback::read_feedback(work, "s1").unwrap();

    assert!(hand_back_held_stage(work, "s1", &[]).unwrap());

    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(stage.held);
    assert_eq!(feedback::read_feedback(work, "s1").unwrap(), notice);
}
