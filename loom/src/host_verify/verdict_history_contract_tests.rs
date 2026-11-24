//! Contract for E4: an unreadable completion record newer than a passing one
//! refuses reuse; it is never skipped so the older pass reads as the newest.

use chrono::Utc;

use super::store::job_dir;
use super::tests_complete_support::{Fixture, STAGE};
use super::verdict_complete::{submit_verdict_completion, VerdictStart};
use super::{JobKind, JobState};
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

#[test]
fn an_unreadable_newer_completion_record_refuses_reuse() {
    let fx = held();
    let older = fx.passed();
    let newer = fx.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", false)],
    );
    assert!(
        newer.queued_at >= older.queued_at,
        "the failed run is newer"
    );
    let record = job_dir(&fx.work_dir, STAGE, &newer.id)
        .expect("the newer job's directory")
        .join("record.json");
    assert!(record.is_file(), "the newer run left {}", record.display());
    std::fs::write(&record, b"\x00 not a job record {").expect("corrupt the newer record");

    let start = submit_verdict_completion(&fx.work_dir, &fx.load_stage(), Utc::now());
    if let Ok(VerdictStart::Reused { job, commit }) = &start {
        panic!("reused job {job} at {commit} past an unreadable newer completion record");
    }
    assert_ne!(
        fx.load_stage().status,
        StageStatus::Completed,
        "the stage completed from the older pass: {:?}",
        start.map(|_| ())
    );
}
