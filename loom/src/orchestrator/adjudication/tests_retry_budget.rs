//! Failure paths for the durable verdict application budget.

use super::tests::{make_stage, reject_verdict, write_dispute_request, write_stage, write_verdict};
use super::{counter, feedback, AdjudicatorRegistry};
use crate::models::stage::StageStatus;

fn ready_verdict() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(work, "s1", 1, reject_verdict(), 1);
    tmp
}

fn assert_unapplied(work: &std::path::Path) {
    assert_eq!(
        crate::verify::transitions::load_stage("s1", work)
            .unwrap()
            .status,
        StageStatus::NeedsAdjudication
    );
    assert!(!work.join("disputes/s1/1/applied.marker").exists());
    assert!(feedback::read_feedback(work, "s1").unwrap().is_none());
}

#[test]
fn corrupt_apply_budget_stops_verdict_before_effects() {
    let tmp = ready_verdict();
    let work = tmp.path();
    let counter = work.join("disputes/s1/1/apply_failures");
    std::fs::write(&counter, "broken").unwrap();

    assert!(AdjudicatorRegistry::new()
        .apply_pending_verdicts(work)
        .is_err());
    assert_unapplied(work);
}

#[test]
fn failed_apply_budget_write_stops_effects_and_retry_remains_bounded() {
    let tmp = ready_verdict();
    let work = tmp.path();
    let registry = AdjudicatorRegistry::new();
    let result = counter::with_write_failure(|| registry.apply_pending_verdicts(work));
    assert!(result.is_err());
    assert_unapplied(work);
    assert!(!work.join("disputes/s1/1/apply_failures").exists());

    registry.apply_verdict(work, "s1", 1).unwrap();
    assert_eq!(
        std::fs::read_to_string(work.join("disputes/s1/1/apply_failures")).unwrap(),
        "1"
    );
    assert_eq!(
        crate::verify::transitions::load_stage("s1", work)
            .unwrap()
            .status,
        StageStatus::NeedsHumanReview
    );
}

#[test]
fn unreadable_request_retry_budget_survives_registry_restart() {
    let tmp = ready_verdict();
    let work = tmp.path();
    std::fs::write(work.join("disputes/s1/1/request.md"), "invalid request").unwrap();

    for expected in 1..super::MAX_APPLY_ATTEMPTS {
        assert!(AdjudicatorRegistry::new()
            .apply_verdict(work, "s1", 1)
            .is_err());
        assert_eq!(
            std::fs::read_to_string(work.join("disputes/s1/1/apply_failures")).unwrap(),
            expected.to_string()
        );
        assert_unapplied(work);
    }
    AdjudicatorRegistry::new()
        .apply_verdict(work, "s1", 1)
        .unwrap();
    assert!(work.join("disputes/s1/1/applied.marker").exists());
    assert_eq!(
        crate::verify::transitions::load_stage("s1", work)
            .unwrap()
            .status,
        StageStatus::NeedsHumanReview
    );
}
