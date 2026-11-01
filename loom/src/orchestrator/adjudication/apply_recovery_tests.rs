use super::super::apply_receipt::fail_marker_once;
use super::super::tests::{
    make_stage, reject_verdict, write_dispute_request, write_request, write_stage, write_verdict,
};
use super::super::AdjudicatorRegistry;
use crate::models::dispute::{DisputeKind, DisputeVerdict};
use crate::models::stage::StageStatus;
use crate::verify::transitions::load_stage;

fn arrange(kind: DisputeKind) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_request(work, "s1", 1, kind);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".into()],
        },
        1,
    );
    tmp
}

#[test]
fn marker_failure_replay_charges_each_question_path_once() {
    for kind in [
        DisputeKind::criterion(crate::models::dispute::CriterionField::Acceptance, vec![0]),
        DisputeKind::FreezeRefusal {
            problems: vec!["freeze refused".into()],
        },
    ] {
        let tmp = arrange(kind);
        let work = tmp.path();
        let registry = AdjudicatorRegistry::new();
        let marker = work.join("disputes/s1/1/applied.marker");
        fail_marker_once();
        let error = registry.apply_verdict(work, "s1", 1).unwrap_err();
        assert!(format!("{error:#}").contains("injected applied-marker"));
        assert!(!marker.exists());
        let charged = load_stage("s1", work).unwrap();
        assert_eq!(charged.tally.evidence_rounds, 1);
        assert!(charged
            .tally
            .request_receipts
            .contains_key("adjudication-verdict:1"));

        for _ in 0..3 {
            registry.apply_verdict(work, "s1", 1).unwrap();
            assert_eq!(load_stage("s1", work).unwrap().tally.evidence_rounds, 1);
            std::fs::remove_file(&marker).unwrap();
        }
        registry.apply_verdict(work, "s1", 1).unwrap();
        assert!(marker.exists());
    }
}

#[test]
fn distinct_verdicts_each_charge_one_round() {
    let tmp = arrange(DisputeKind::criterion(
        crate::models::dispute::CriterionField::Acceptance,
        vec![0],
    ));
    let work = tmp.path();
    write_dispute_request(work, "s1", 2, 0);
    let registry = AdjudicatorRegistry::new();
    registry.apply_verdict(work, "s1", 1).unwrap();
    assert_eq!(
        load_stage("s1", work).unwrap().status,
        StageStatus::NeedsAdjudication
    );

    write_verdict(
        work,
        "s1",
        2,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["also why?".into()],
        },
        1,
    );
    registry.apply_verdict(work, "s1", 2).unwrap();
    let after = load_stage("s1", work).unwrap();
    assert_eq!(after.tally.evidence_rounds, 2);
    assert_eq!(after.status, StageStatus::Queued);
    assert!(after
        .tally
        .request_receipts
        .contains_key("adjudication-verdict:1"));
    assert!(after
        .tally
        .request_receipts
        .contains_key("adjudication-verdict:2"));
}

#[test]
fn two_recorded_verdicts_hold_until_both_are_applied() {
    let tmp = arrange(DisputeKind::criterion(
        crate::models::dispute::CriterionField::Acceptance,
        vec![0],
    ));
    let work = tmp.path();
    write_dispute_request(work, "s1", 2, 0);
    write_verdict(
        work,
        "s1",
        2,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["also why?".into()],
        },
        1,
    );
    let registry = AdjudicatorRegistry::new();
    registry.apply_verdict(work, "s1", 1).unwrap();
    assert_eq!(
        load_stage("s1", work).unwrap().status,
        StageStatus::NeedsAdjudication
    );
    registry.apply_verdict(work, "s1", 2).unwrap();
    let after = load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert_eq!(after.tally.evidence_rounds, 2);
}

#[test]
fn a_marker_failed_sibling_does_not_strand_the_last_verdict() {
    let tmp = arrange(DisputeKind::criterion(
        crate::models::dispute::CriterionField::Acceptance,
        vec![0],
    ));
    let work = tmp.path();
    write_dispute_request(work, "s1", 2, 0);
    write_verdict(
        work,
        "s1",
        2,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["also why?".into()],
        },
        1,
    );
    let registry = AdjudicatorRegistry::new();
    fail_marker_once();
    registry.apply_verdict(work, "s1", 1).unwrap_err();
    assert_eq!(
        load_stage("s1", work).unwrap().status,
        StageStatus::NeedsAdjudication
    );

    registry.apply_verdict(work, "s1", 2).unwrap();
    assert_eq!(load_stage("s1", work).unwrap().status, StageStatus::Queued);
    registry.apply_verdict(work, "s1", 1).unwrap();
    let after = load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert_eq!(after.tally.evidence_rounds, 2);
    assert!(work.join("disputes/s1/1/applied.marker").exists());
    assert!(work.join("disputes/s1/2/applied.marker").exists());
}

#[test]
fn cap_receipt_recovers_marker_and_closure_after_crash() {
    let tmp = arrange(DisputeKind::criterion(
        crate::models::dispute::CriterionField::Acceptance,
        vec![0],
    ));
    let work = tmp.path();
    write_dispute_request(work, "s1", 2, 0);
    crate::verify::transitions::update_stage("s1", work, |stage| {
        stage.status = StageStatus::NeedsHumanReview;
        stage
            .tally
            .request_receipts
            .insert("adjudication-apply-cap:1".into(), 1);
        Ok(())
    })
    .unwrap();
    let registry = AdjudicatorRegistry::new();
    let marker = work.join("disputes/s1/1/applied.marker");
    let closed = work.join("disputes/s1/1/closed.marker");
    let sibling_closed = work.join("disputes/s1/2/closed.marker");
    fail_marker_once();
    assert!(registry.apply_verdict(work, "s1", 1).is_err());
    assert!(!marker.exists());
    assert!(!closed.exists());
    assert!(sibling_closed.exists());

    registry.apply_verdict(work, "s1", 1).unwrap();
    assert!(marker.exists());
    assert!(sibling_closed.exists());
    assert!(
        !closed.exists(),
        "applied dispute remains distinct from abandoned siblings"
    );
    assert_eq!(
        load_stage("s1", work).unwrap().status,
        StageStatus::NeedsHumanReview
    );
}

#[test]
fn rejecting_sibling_recovers_committed_question_marker_before_closure() {
    let tmp = arrange(DisputeKind::criterion(
        crate::models::dispute::CriterionField::Acceptance,
        vec![0],
    ));
    let work = tmp.path();
    write_dispute_request(work, "s1", 2, 0);
    let registry = AdjudicatorRegistry::new();
    fail_marker_once();
    registry.apply_verdict(work, "s1", 1).unwrap_err();
    let first_marker = work.join("disputes/s1/1/applied.marker");
    let first_closed = work.join("disputes/s1/1/closed.marker");
    assert!(!first_marker.exists());

    write_verdict(work, "s1", 2, reject_verdict(), 1);
    registry.apply_verdict(work, "s1", 2).unwrap();
    assert!(
        first_marker.exists(),
        "committed sibling must be marked applied"
    );
    assert!(
        !first_closed.exists(),
        "committed sibling must not be abandoned"
    );
    assert_eq!(
        load_stage("s1", work).unwrap().status,
        StageStatus::NeedsHumanReview
    );
}
