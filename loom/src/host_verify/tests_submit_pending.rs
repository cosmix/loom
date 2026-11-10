//! Waiting selections, supersession and crash-safe slot recovery.
use super::submit::{cancel, cancel_if_id, submit, Submission, SubmitRequest};
use super::tests_submit::*;
use super::{pending, store, JobKind, JobRecord, JobState};
use chrono::Utc;
use std::path::Path;

fn pair(work: &Path) -> (JobRecord, JobRecord) {
    let current = queued(submit(work, &subset("r1", &[1]), Utc::now()).unwrap());
    let (pending, behind) = waiting(submit(work, &subset("r2", &[2]), Utc::now()).unwrap());
    assert_eq!(behind, current.id);
    (current, pending)
}

fn assert_cancelled(work: &Path, job: &JobRecord, reason: &str) {
    let record = store::load_job_record(work, STAGE, &job.id)
        .unwrap()
        .unwrap();
    assert_eq!(record.state, JobState::Cancelled);
    assert_eq!(record.detail.as_deref(), Some(reason));
}

#[test]
fn two_checks_on_other_criteria_of_one_tree_never_cancel_each_other() {
    let (_tmp, work) = state_root();
    let (current, waiting) = pair(&work);
    assert_eq!(current.state, JobState::Queued);
    assert_eq!(current.detail, None);
    assert_eq!(store::load_current(&work, STAGE).unwrap(), Some(current));
    assert_eq!(waiting.criteria, Some(vec![2]));
    assert_eq!(waiting.requests, ["r2"]);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), Some(waiting));
}

#[test]
fn waiting_selections_merge_into_one_waiting_job() {
    let (_tmp, work) = state_root();
    let (_, initial) = pair(&work);
    for (id, indices) in [("r3", vec![3]), ("r4", vec![2])] {
        let (next, _) = waiting(submit(&work, &subset(id, &indices), Utc::now()).unwrap());
        assert_eq!(next.id, initial.id);
    }
    let next = store::load_pending(&work, STAGE).unwrap().unwrap();
    assert_eq!(next.criteria, Some(vec![2, 3]));
    assert_eq!(next.requests, ["r2", "r3", "r4"]);
    let (full, _) = waiting(submit(&work, &request("r5", HEAD_A, false), Utc::now()).unwrap());
    assert_eq!(full.id, initial.id);
    assert_eq!(full.criteria, None);
    let (uncached, _) = waiting(submit(&work, &request("r6", HEAD_A, true), Utc::now()).unwrap());
    assert_eq!(uncached.id, initial.id);
    assert!(uncached.no_cache);
}

#[test]
fn a_request_the_running_job_covers_still_joins_it() {
    let (_tmp, work) = state_root();
    let mut current = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());
    current.state = JobState::Running;
    store::save_current(&work, &current).unwrap();
    let job = joined(submit(&work, &subset("r2", &[1]), Utc::now()).unwrap());
    assert_eq!(job.id, current.id);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn a_new_tree_supersedes_the_running_check_and_its_waiting_one() {
    let (_tmp, work) = state_root();
    let (mut current, waiting) = pair(&work);
    current.state = JobState::Running;
    store::save_current(&work, &current).unwrap();
    let (_, next) = superseded(submit(&work, &request("r3", HEAD_B, false), Utc::now()).unwrap());
    let reason = format!("superseded by {}", next.id);
    assert_cancelled(&work, &current, &reason);
    assert_cancelled(&work, &waiting, &reason);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
    assert_eq!(store::load_current(&work, STAGE).unwrap(), Some(next));
}

#[test]
fn another_session_on_the_same_tree_still_supersedes() {
    let (_tmp, work) = state_root();
    queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    let request = SubmitRequest {
        session_id: "session-b",
        ..subset("r2", &[2])
    };
    superseded(submit(&work, &request, Utc::now()).unwrap());
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn a_completion_supersedes_a_check_and_its_waiting_check() {
    let (_tmp, work) = state_root();
    let (current, waiting) = pair(&work);
    let request = SubmitRequest {
        kind: JobKind::Complete,
        ..request("r3", HEAD_A, false)
    };
    let (_, next) = superseded(submit(&work, &request, Utc::now()).unwrap());
    let reason = format!("superseded by {}", next.id);
    assert_cancelled(&work, &current, &reason);
    assert_cancelled(&work, &waiting, &reason);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn a_check_never_waits_behind_a_completion() {
    let (_tmp, work) = state_root();
    let complete = SubmitRequest {
        kind: JobKind::Complete,
        ..request("r1", HEAD_A, false)
    };
    queued(submit(&work, &complete, Utc::now()).unwrap());
    assert!(
        matches!(submit(&work, &request("r2", HEAD_A, true), Utc::now()).unwrap(), Submission::Refused(reason) if reason.contains("completion or contract"))
    );
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn cancel_ends_the_waiting_check_with_the_current_one() {
    let (_tmp, work) = state_root();
    let (current, waiting) = pair(&work);
    let cancelled = cancel(&work, STAGE, "stage agents taken down")
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.id, current.id);
    assert_cancelled(&work, &current, "stage agents taken down");
    assert_cancelled(&work, &waiting, "stage agents taken down");
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn cancel_ends_a_waiting_check_whose_current_job_already_ended() {
    let (_tmp, work) = state_root();
    let (mut current, waiting) = pair(&work);
    current.state = JobState::Passed;
    store::save_current(&work, &current).unwrap();
    let cancelled = cancel(&work, STAGE, "taken down").unwrap().unwrap();
    assert_eq!(cancelled.id, waiting.id);
    assert_cancelled(&work, &waiting, "taken down");
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn cancel_if_id_ends_the_waiting_check_behind_the_observed_job() {
    let (_tmp, work) = state_root();
    let (current, waiting) = pair(&work);
    assert_eq!(cancel_if_id(&work, STAGE, "other", "reset").unwrap(), None);
    assert_eq!(
        store::load_current(&work, STAGE).unwrap(),
        Some(current.clone())
    );
    assert_eq!(
        store::load_pending(&work, STAGE).unwrap(),
        Some(waiting.clone())
    );
    cancel_if_id(&work, STAGE, &current.id, "reset").unwrap();
    assert_cancelled(&work, &current, "reset");
    assert_cancelled(&work, &waiting, "reset");
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn a_submit_after_the_current_job_ended_promotes_the_waiting_check_first() {
    let (_tmp, work) = state_root();
    let (mut current, waiting) = pair(&work);
    current.state = JobState::Passed;
    store::save_current(&work, &current).unwrap();
    let joined = joined(submit(&work, &subset("r9", &[2]), Utc::now()).unwrap());
    assert_eq!(joined.id, waiting.id);
    assert_eq!(joined.state, JobState::Queued);
    assert_eq!(store::load_current(&work, STAGE).unwrap(), Some(joined));
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn a_waiting_slot_whose_copy_was_cancelled_is_dropped() {
    let (_tmp, work) = state_root();
    let (current, mut waiting) = pair(&work);
    waiting.state = JobState::Cancelled;
    store::save_job_copy(&work, &waiting).unwrap();
    assert_eq!(pending::promote(&work, STAGE).unwrap(), None);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
    assert_eq!(store::load_current(&work, STAGE).unwrap(), Some(current));
}

#[test]
fn a_promotion_cut_short_after_saving_current_only_clears_the_slot() {
    let (_tmp, work) = state_root();
    let (_, waiting) = pair(&work);
    store::save_current(&work, &waiting).unwrap();
    assert_eq!(
        pending::promote(&work, STAGE).unwrap(),
        Some(waiting.clone())
    );
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
    assert_eq!(store::load_current(&work, STAGE).unwrap(), Some(waiting));
}

#[test]
fn a_submit_after_interrupted_promotion_creates_a_distinct_waiting_job() {
    let (_tmp, work) = state_root();
    let (_, promoted) = pair(&work);
    // Crash after saving current, before clearing the old waiting pointer.
    store::save_current(&work, &promoted).unwrap();
    let (next, behind) = waiting(submit(&work, &subset("r3", &[3]), Utc::now()).unwrap());
    assert_eq!(behind, promoted.id);
    assert_ne!(next.id, promoted.id);
    assert_eq!(next.criteria, Some(vec![3]));
    assert_eq!(
        store::load_current(&work, STAGE).unwrap(),
        Some(promoted.clone())
    );
    assert_eq!(
        store::load_pending(&work, STAGE).unwrap(),
        Some(next.clone())
    );
    let mut ended = promoted;
    ended.state = JobState::Passed;
    store::save_current(&work, &ended).unwrap();
    assert_eq!(pending::promote(&work, STAGE).unwrap(), Some(next));
}

#[test]
fn a_changed_tree_on_the_same_head_supersedes_the_check_and_its_waiting_one() {
    let (_tmp, work) = state_root();
    let (mut current, waiting) = pair(&work);
    current.state = JobState::Running;
    store::save_current(&work, &current).unwrap();
    let digest = "d".repeat(64);
    let edited = SubmitRequest {
        tree_digest: &digest,
        ..subset("r3", &[3])
    };
    let (_, next) = superseded(submit(&work, &edited, Utc::now()).unwrap());
    let reason = format!("superseded by {}", next.id);
    assert_cancelled(&work, &current, &reason);
    assert_cancelled(&work, &waiting, &reason);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
    assert_eq!(store::load_current(&work, STAGE).unwrap(), Some(next));
}

#[test]
fn widening_never_narrows_a_full_waiting_job_or_drops_its_no_cache() {
    let (_tmp, work) = state_root();
    let current = queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    let (full, behind) = waiting(submit(&work, &request("r2", HEAD_A, true), Utc::now()).unwrap());
    assert_eq!(behind, current.id);
    assert_eq!((full.criteria.clone(), full.no_cache), (None, true));
    let (widened, _) = waiting(submit(&work, &subset("r3", &[4]), Utc::now()).unwrap());
    assert_eq!(widened.id, full.id);
    assert_eq!((widened.criteria, widened.no_cache), (None, true));
}
