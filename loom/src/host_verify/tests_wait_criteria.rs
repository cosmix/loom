//! A session's request is followed only by a call selecting the same
//! criteria: a subset job never answers a full check, and a full check never
//! follows a subset job.

use std::time::Duration;

use super::store::save_current;
use super::tests_wait::{fixture, Fx};
use super::wait::{wait_or_request, WaitOutcome, WaitSite};
use super::JobState;
use crate::relay::HostJobRequest;

const JOB: &str = "33333333333333333333333333333333";

/// One call selecting `criteria`, relaying `request_id`; returns the outcome
/// and the criteria of the ticket it relayed, if it relayed one.
fn call(
    fx: &Fx,
    criteria: Option<&[usize]>,
    request_id: &str,
) -> (WaitOutcome, Option<Option<Vec<usize>>>) {
    let site = WaitSite {
        criteria,
        ..fx.site(Duration::ZERO, false)
    };
    let mut ticket = None;
    let mut relay = |request: &HostJobRequest| {
        ticket = Some(request.criteria.clone());
        Ok(request_id.to_string())
    };
    let outcome = wait_or_request(&site, &mut relay, &mut Vec::new()).unwrap();
    (outcome, ticket)
}

fn state_names(fx: &Fx, request_id: &str) -> bool {
    std::fs::read_to_string(fx.state_file())
        .unwrap()
        .contains(request_id)
}

#[test]
fn a_full_check_never_follows_a_pending_subset_job() {
    let fx = fixture();
    let (first, ticket) = call(&fx, Some(&[1]), "r1");
    assert_eq!(first, WaitOutcome::Requested);
    assert_eq!(
        ticket,
        Some(Some(vec![1])),
        "the subset rides on the ticket"
    );
    let mut subset = fx.job(JOB, JobState::Running, &["r1"]);
    subset.criteria = Some(vec![1]);
    save_current(&fx.work_dir, &subset).unwrap();

    let (same, ticket) = call(&fx, Some(&[1]), "r9");
    assert_eq!(same, WaitOutcome::Pending { done: 0 });
    assert_eq!(ticket, None, "the same selection follows its request");

    let (full, ticket) = call(&fx, None, "r2");
    assert_eq!(full, WaitOutcome::Requested);
    assert_eq!(ticket, Some(None), "a full check requests every criterion");
    assert!(state_names(&fx, "r2"));
}

#[test]
fn a_subset_check_requests_its_own_job_after_a_full_request() {
    let fx = fixture();
    let (first, ticket) = call(&fx, None, "r1");
    assert_eq!(first, WaitOutcome::Requested);
    assert_eq!(ticket, Some(None));
    save_current(&fx.work_dir, &fx.job(JOB, JobState::Running, &["r1"])).unwrap();

    let (subset, ticket) = call(&fx, Some(&[0, 2]), "r2");
    assert_eq!(subset, WaitOutcome::Requested);
    assert_eq!(ticket, Some(Some(vec![0, 2])));
    assert!(state_names(&fx, "r2"));
}
