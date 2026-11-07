//! A finished freeze job outlives its request state until the caller releases
//! it: only the daemon's application of the job decides the freeze.

use std::path::PathBuf;
use std::time::Duration;

use crate::host_verify::store::{save_current, save_result};
use crate::host_verify::tests_wait::{fixture, Fx};
use crate::host_verify::wait::{
    exit_code, pending_request_id, release, wait_or_request, WaitOutcome, WaitSite,
};
use crate::host_verify::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::relay::HostJobRequest;

const JOB: &str = "44444444444444444444444444444444";

fn site_of(fx: &Fx, kind: JobKind) -> WaitSite<'_> {
    WaitSite {
        kind,
        ..fx.site(Duration::ZERO, false)
    }
}

fn job_of(fx: &Fx, kind: JobKind, state: JobState) -> JobRecord {
    JobRecord {
        kind,
        ..fx.job(JOB, state, &["r1"])
    }
}

fn state_file(fx: &Fx, kind: JobKind) -> PathBuf {
    fx.state_file()
        .with_file_name(format!("s1-{}.json", kind.name()))
}

fn call(fx: &Fx, kind: JobKind) -> (WaitOutcome, String) {
    let mut out = Vec::new();
    let mut relay = |_: &HostJobRequest| Ok("r1".to_string());
    let outcome = wait_or_request(&site_of(fx, kind), &mut relay, &mut out).unwrap();
    (outcome, String::from_utf8(out).unwrap())
}

/// Request "r1" for `kind`, then leave a job in `state` that holds it, with a
/// failing criterion when `state` is `Failed`.
fn finished(fx: &Fx, kind: JobKind, state: JobState) -> JobRecord {
    call(fx, kind);
    let record = job_of(fx, kind, state);
    save_current(&fx.work_dir, &record).unwrap();
    let criteria = vec![CriterionOutcome {
        label: "format".to_string(),
        passed: state == JobState::Passed,
        summary: "src/lib.rs needs formatting".to_string(),
        log: None,
    }];
    let result = JobResult {
        job_id: record.id.clone(),
        head: record.head.clone(),
        passed: state == JobState::Passed,
        criteria,
        error: None,
        policy_hash: None,
    };
    save_result(&fx.work_dir, "s1", &result).unwrap();
    record
}

#[test]
fn a_failed_freeze_job_is_reported_and_keeps_its_request_state() {
    let fx = fixture();
    let record = finished(&fx, JobKind::Freeze, JobState::Failed);

    let (outcome, text) = call(&fx, JobKind::Freeze);
    assert!(matches!(&outcome, WaitOutcome::Finished(job, _) if job.id == record.id));
    assert_eq!(exit_code(&outcome), 1);
    assert!(text.starts_with("Host job: failed (freeze, "), "{text}");
    assert!(text.contains("src/lib.rs needs formatting"), "{text}");
    assert!(state_file(&fx, JobKind::Freeze).is_file());

    release(&site_of(&fx, JobKind::Freeze), &record).unwrap();
    assert!(!state_file(&fx, JobKind::Freeze).exists());
}

#[test]
fn a_passed_freeze_job_is_reported_and_keeps_its_request_state() {
    let fx = fixture();
    finished(&fx, JobKind::Freeze, JobState::Passed);

    let (outcome, text) = call(&fx, JobKind::Freeze);
    assert_eq!(exit_code(&outcome), 0);
    assert!(text.starts_with("Host job: passed (freeze, "), "{text}");
    assert!(state_file(&fx, JobKind::Freeze).is_file());
}

#[test]
fn a_stopped_freeze_job_and_other_kinds_clear_the_state_as_before() {
    let fx = fixture();
    finished(&fx, JobKind::Freeze, JobState::Interrupted);
    let (outcome, _) = call(&fx, JobKind::Freeze);
    assert_eq!(exit_code(&outcome), 4);
    assert!(!state_file(&fx, JobKind::Freeze).exists());

    finished(&fx, JobKind::Check, JobState::Failed);
    call(&fx, JobKind::Check);
    assert!(!state_file(&fx, JobKind::Check).exists());

    finished(&fx, JobKind::Complete, JobState::Failed);
    call(&fx, JobKind::Complete);
    assert!(!state_file(&fx, JobKind::Complete).exists());
}

#[test]
fn pending_request_id_names_the_state_a_call_would_follow() {
    let fx = fixture();
    let site = site_of(&fx, JobKind::Freeze);
    assert_eq!(pending_request_id(&site).unwrap(), None);

    call(&fx, JobKind::Freeze);
    assert_eq!(pending_request_id(&site).unwrap().as_deref(), Some("r1"));

    std::fs::write(fx.checkout.join("edited.txt"), "another tree\n").unwrap();
    assert_eq!(pending_request_id(&site).unwrap(), None);
}
