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

fn leave_adjudication(work: &std::path::Path) {
    crate::verify::transitions::update_stage("s1", work, |stage| {
        stage.status = StageStatus::Queued;
        Ok(())
    })
    .unwrap();
}

/// Reading the verdict again can never change a stage that left adjudication,
/// whatever the budget says: without a closure it stays pending every tick and
/// its judge is never retired.
#[test]
fn a_verdict_for_a_stage_that_left_adjudication_closes_uncharged() {
    for spent in [None, Some(super::MAX_APPLY_ATTEMPTS)] {
        let tmp = ready_verdict();
        let work = tmp.path();
        leave_adjudication(work);
        let counter = work.join("disputes/s1/1/apply_failures");
        if let Some(spent) = spent {
            std::fs::write(&counter, spent.to_string()).unwrap();
        }
        let before = std::fs::read(&counter).ok();
        let registry = AdjudicatorRegistry::new();

        registry.apply_verdict(work, "s1", 1).unwrap();

        assert!(work.join("disputes/s1/1/closed.marker").exists());
        assert!(!work.join("disputes/s1/1/applied.marker").exists());
        assert_eq!(
            std::fs::read(&counter).ok(),
            before,
            "no attempt is charged"
        );
        let after = crate::verify::transitions::load_stage("s1", work).unwrap();
        assert_eq!(after.status, StageStatus::Queued);
        assert!(after.tally.request_receipts.is_empty());
        assert!(registry.pending_verdicts(work).unwrap().is_empty());
    }
}

/// A sibling dispute keeps its own lifecycle: only the dispute being applied
/// is closed.
#[test]
fn closing_a_stale_verdict_leaves_sibling_disputes_open() {
    let tmp = ready_verdict();
    let work = tmp.path();
    write_dispute_request(work, "s1", 2, 0);
    leave_adjudication(work);

    AdjudicatorRegistry::new()
        .apply_verdict(work, "s1", 1)
        .unwrap();

    assert!(work.join("disputes/s1/1/closed.marker").exists());
    assert!(!work.join("disputes/s1/2/closed.marker").exists());
}
