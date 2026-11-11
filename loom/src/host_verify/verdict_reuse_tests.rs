//! A verdict that changes nothing completes the held stage from its latest
//! `loom stage complete` run when that run passed on the same head under the
//! same check definition and the gates pass on the stored state; anything
//! else keeps the stage-bound run or hands the stage back.

use chrono::Utc;

use crate::host_verify::apply::APPLIED_MARKER;
use crate::host_verify::tests_complete_support::{Fixture, STAGE};
use crate::host_verify::verdict_complete::{
    submit_verdict_completion, VerdictStart, STAGE_BOUND_SESSION,
};
use crate::host_verify::{store, JobKind, JobRecord, JobState};
use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::update_stage;

/// A plan v2 standard stage a verdict held, its review current.
fn held() -> Fixture {
    let fx = Fixture::new(StageType::Standard, 2);
    fx.record_review(false);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.status = StageStatus::NeedsAdjudication;
        stage.session = None;
        Ok(())
    })
    .expect("hold the stage");
    fx
}

fn submit(fx: &Fixture) -> VerdictStart {
    submit_verdict_completion(&fx.work_dir, &fx.load_stage(), Utc::now())
        .expect("submit the verdict completion")
}

fn current(fx: &Fixture) -> JobRecord {
    store::load_current(&fx.work_dir, STAGE)
        .expect("load the current record")
        .expect("a current record")
}

#[test]
fn a_passing_run_on_the_same_tree_completes_the_stage_without_a_job() {
    let fx = held();
    let passed = fx.passed();

    let start = submit(&fx);

    assert_eq!(
        start,
        VerdictStart::Reused {
            job: passed.id.clone(),
            commit: passed.head.clone()
        }
    );
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(
        stage.completed_commit.as_deref(),
        Some(passed.head.as_str())
    );
    assert_eq!(
        current(&fx).id,
        passed.id,
        "no verdict completion job queued"
    );
}

#[test]
fn a_run_that_failed_its_impact_tests_is_run_again() {
    let fx = held();
    fx.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", true), ("impact-tests", false)],
    );

    let start = submit(&fx);

    let VerdictStart::Submitted(id) = start else {
        panic!("expected Submitted, got {start:?}");
    };
    let job = current(&fx);
    assert_eq!((job.id, job.kind), (id, JobKind::VerdictComplete));
    assert_eq!(fx.load_stage().status, StageStatus::NeedsAdjudication);
}

#[test]
fn a_later_verdict_run_outranks_the_passing_run() {
    // Both records predate submission ordering, so the later queue time ranks.
    let fx = held();
    let passed = fx.passed();
    let mut rerun = fx.finished(
        JobKind::VerdictComplete,
        JobState::Failed,
        &[("acceptance-0", false)],
    );
    assert!(rerun.queued_at > passed.queued_at);
    rerun.session_id = STAGE_BOUND_SESSION.to_string();
    store::save_current(&fx.work_dir, &rerun).expect("save the later run");
    store::save_job_file(
        &fx.work_dir,
        STAGE,
        &rerun.id,
        APPLIED_MARKER,
        b"handed back",
    )
    .expect("mark the later run applied");

    let start = submit(&fx);

    assert!(matches!(start, VerdictStart::Submitted(_)), "{start:?}");
    assert_eq!(fx.load_stage().status, StageStatus::NeedsAdjudication);
}

#[test]
fn a_stale_review_refuses_the_stage_without_a_job() {
    let fx = held();
    fx.record_review(true);
    let passed = fx.passed();

    let start = submit(&fx);

    let VerdictStart::GatesRefused(refusal) = start else {
        panic!("expected GatesRefused, got {start:?}");
    };
    assert!(refusal.starts_with("completion gates: "), "{refusal}");
    assert!(refusal.contains("review gate"), "{refusal}");
    assert_eq!(
        current(&fx).id,
        passed.id,
        "no verdict completion job queued"
    );
    assert_eq!(fx.load_stage().status, StageStatus::NeedsAdjudication);
}
