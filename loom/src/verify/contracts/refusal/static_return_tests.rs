//! The count of static refusals a writer session gets back before one is
//! filed, kept in `refused.json`.

use tempfile::TempDir;

use super::super::{daemon_refusal, mark_declined, record_job_refusal, JobRefusalOrigin};
use super::*;

const STAGE: &str = "s1";

fn origin(job: &str, order: u64) -> JobRefusalOrigin {
    JobRefusalOrigin {
        job_id: job.to_string(),
        submission_order: order,
    }
}

fn record(work: &TempDir, session: &str, job: &str, order: u64) -> StaticRefusal {
    let problems = vec![format!("format: {job} is not formatted")];
    record_static_refusal(work.path(), STAGE, session, origin(job, order), &problems).unwrap()
}

fn returned(work: &TempDir) -> u32 {
    daemon_refusal(work.path(), STAGE)
        .unwrap()
        .expect("a refusal is on record")
        .returned
}

#[test]
fn the_third_static_refusal_of_a_session_is_filed() {
    let work = TempDir::new().unwrap();
    assert_eq!(
        record(&work, "w1", "j1", 1),
        StaticRefusal::Returned { count: 1 }
    );
    assert_eq!(returned(&work), 1);
    assert_eq!(
        record(&work, "w1", "j2", 2),
        StaticRefusal::Returned { count: 2 }
    );
    assert_eq!(returned(&work), 2);
    assert_eq!(record(&work, "w1", "j3", 3), StaticRefusal::Filed);
    assert_eq!(returned(&work), 0, "a filed refusal stands at 0");
    let on_record = daemon_refusal(work.path(), STAGE).unwrap().unwrap();
    assert_eq!(on_record.problems, ["format: j3 is not formatted"]);
    assert_eq!(on_record.job_origin, Some(origin("j3", 3)));
}

#[test]
fn the_same_job_applied_twice_counts_once() {
    let work = TempDir::new().unwrap();
    assert_eq!(
        record(&work, "w1", "j1", 1),
        StaticRefusal::Returned { count: 1 }
    );
    assert_eq!(
        record(&work, "w1", "j1", 1),
        StaticRefusal::Returned { count: 1 }
    );
    assert_eq!(returned(&work), 1);
    assert_eq!(
        record(&work, "w1", "j2", 2),
        StaticRefusal::Returned { count: 2 }
    );
}

#[test]
fn an_older_job_is_superseded_and_changes_nothing() {
    let work = TempDir::new().unwrap();
    record(&work, "w1", "j5", 5);
    assert_eq!(record(&work, "w1", "j4", 4), StaticRefusal::Superseded);
    let on_record = daemon_refusal(work.path(), STAGE).unwrap().unwrap();
    assert_eq!(on_record.job_origin, Some(origin("j5", 5)));
    assert_eq!(on_record.returned, 1);
}

#[test]
fn another_session_starts_again_at_one() {
    let work = TempDir::new().unwrap();
    record(&work, "w1", "j1", 1);
    record(&work, "w1", "j2", 2);
    assert_eq!(
        record(&work, "w2", "j3", 3),
        StaticRefusal::Returned { count: 1 }
    );
    let on_record = daemon_refusal(work.path(), STAGE).unwrap().unwrap();
    assert_eq!(on_record.session_id, "w2");
    assert_eq!(on_record.returned, 1);
}

#[test]
fn a_job_refusal_is_recorded_with_returned_zero() {
    let work = TempDir::new().unwrap();
    record(&work, "w1", "j1", 1);
    let problems = vec!["contract:c1: passed".to_string()];
    assert!(record_job_refusal(work.path(), STAGE, "w1", origin("j2", 2), &problems).unwrap());
    assert_eq!(returned(&work), 0, "a non-static refusal is filed at once");
    let raw = std::fs::read_to_string(work.path().join("contracts/s1/refused.json")).unwrap();
    assert!(!raw.contains("returned"), "0 is not written: {raw}");
}

#[test]
fn a_refusal_still_to_be_filed_is_kept_and_a_newer_static_one_is_filed_with_it() {
    let work = TempDir::new().unwrap();
    record(&work, "w1", "j1", 1);
    record(&work, "w1", "j2", 2);
    assert_eq!(record(&work, "w1", "j3", 3), StaticRefusal::Filed);
    assert_eq!(record(&work, "w1", "j4", 4), StaticRefusal::Filed);
    let on_record = daemon_refusal(work.path(), STAGE).unwrap().unwrap();
    assert_eq!(on_record.problems, ["format: j3 is not formatted"]);
    assert_eq!(on_record.returned, 0, "the count does not start again");

    let problems = vec!["contract:c1: passed".to_string()];
    assert!(record_job_refusal(work.path(), STAGE, "w1", origin("j5", 5), &problems).unwrap());
    assert_eq!(record(&work, "w1", "j6", 6), StaticRefusal::Filed);
    let on_record = daemon_refusal(work.path(), STAGE).unwrap().unwrap();
    assert_eq!(on_record.problems, problems, "the standing refusal is kept");
}

#[test]
fn a_declined_refusal_gives_way_to_a_newer_static_one() {
    let work = TempDir::new().unwrap();
    let problems = vec!["contract:c1: passed".to_string()];
    assert!(record_job_refusal(work.path(), STAGE, "w1", origin("j1", 1), &problems).unwrap());
    mark_declined(work.path(), STAGE, "the budget is spent").unwrap();

    assert_eq!(
        record(&work, "w1", "j2", 2),
        StaticRefusal::Returned { count: 1 }
    );
}
