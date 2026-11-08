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

#[test]
fn two_selections_of_one_session_keep_two_requests() {
    let fx = fixture();
    assert!(matches!(
        call(&fx, Some(&[1]), "r1").0,
        WaitOutcome::Requested { .. }
    ));
    assert!(matches!(
        call(&fx, Some(&[2]), "r2").0,
        WaitOutcome::Requested { .. }
    ));
    let mut current = fx.job(JOB, JobState::Running, &["r1"]);
    current.criteria = Some(vec![1]);
    save_current(&fx.work_dir, &current).unwrap();
    assert_eq!(
        call(&fx, Some(&[1]), "r3"),
        (WaitOutcome::Pending { done: 0 }, None)
    );
    assert_eq!(
        call(&fx, Some(&[2]), "r4"),
        (WaitOutcome::Pending { done: 0 }, None)
    );
    assert!(state_names(&fx, Some(&[1]), "r1"));
    assert!(state_names(&fx, Some(&[2]), "r2"));
}

#[test]
fn reordered_and_repeated_criteria_follow_one_request() {
    use crate::commands::verify::normalized_criteria;
    let fx = fixture();
    let first = normalized_criteria(&[2, 1]);
    assert!(matches!(
        call(&fx, Some(&first), "r1").0,
        WaitOutcome::Requested { .. }
    ));
    let mut current = fx.job(JOB, JobState::Running, &["r1"]);
    current.criteria = Some(first);
    save_current(&fx.work_dir, &current).unwrap();
    assert_eq!(
        call(&fx, Some(&normalized_criteria(&[1, 2, 1])), "r2"),
        (WaitOutcome::Pending { done: 0 }, None)
    );
}

#[test]
fn a_request_waiting_behind_another_job_follows_the_waiting_record() {
    use super::store::{save_job_copy, save_pending};
    use crate::fs::inbox::LedgerOutcome;
    let fx = fixture();
    call(&fx, Some(&[2]), "r2");
    let mut current = fx.job(JOB, JobState::Running, &["r1"]);
    current.criteria = Some(vec![1]);
    save_current(&fx.work_dir, &current).unwrap();
    let mut waiting = fx.job(
        "44444444444444444444444444444444",
        JobState::Queued,
        &["r2"],
    );
    waiting.criteria = Some(vec![2]);
    save_pending(&fx.work_dir, &waiting).unwrap();
    fx.ledger(
        "r2",
        LedgerOutcome::Applied,
        &format!("host job {} queued behind {}", waiting.id, current.id),
    );
    assert_eq!(
        call(&fx, Some(&[2]), "r3"),
        (WaitOutcome::Pending { done: 0 }, None)
    );
    waiting.state = JobState::Cancelled;
    save_job_copy(&fx.work_dir, &waiting).unwrap();
    let (outcome, ticket) = call(&fx, Some(&[2]), "r4");
    assert!(
        matches!(&outcome, WaitOutcome::Finished(record, _) if record.state == JobState::Cancelled)
    );
    assert_eq!(super::wait::exit_code(&outcome), 4);
    assert_eq!(ticket, None);
    let path = super::wait_state::state_path(&WaitSite {
        criteria: Some(&[2]),
        ..fx.site(Duration::ZERO, false)
    })
    .unwrap();
    assert!(!path.exists());
}

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

fn state_names(fx: &Fx, criteria: Option<&[usize]>, request_id: &str) -> bool {
    std::fs::read_to_string(
        super::wait_state::state_path(&WaitSite {
            criteria,
            ..fx.site(Duration::ZERO, false)
        })
        .unwrap(),
    )
    .unwrap()
    .contains(request_id)
}

#[test]
fn a_full_check_never_follows_a_pending_subset_job() {
    let fx = fixture();
    let (first, ticket) = call(&fx, Some(&[1]), "r1");
    assert_eq!(
        first,
        WaitOutcome::Requested {
            request_id: "r1".into()
        }
    );
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
    assert_eq!(
        full,
        WaitOutcome::Requested {
            request_id: "r2".into()
        }
    );
    assert_eq!(ticket, Some(None), "a full check requests every criterion");
    assert!(state_names(&fx, None, "r2"));
}

#[test]
fn a_subset_check_requests_its_own_job_after_a_full_request() {
    let fx = fixture();
    let (first, ticket) = call(&fx, None, "r1");
    assert_eq!(
        first,
        WaitOutcome::Requested {
            request_id: "r1".into()
        }
    );
    assert_eq!(ticket, Some(None));
    save_current(&fx.work_dir, &fx.job(JOB, JobState::Running, &["r1"])).unwrap();

    let (subset, ticket) = call(&fx, Some(&[0, 2]), "r2");
    assert_eq!(
        subset,
        WaitOutcome::Requested {
            request_id: "r2".into()
        }
    );
    assert_eq!(ticket, Some(Some(vec![0, 2])));
    assert!(state_names(&fx, Some(&[0, 2]), "r2"));
}
