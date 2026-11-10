//! Submitting and cancelling host jobs: queue, join, supersede, cancel.

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use tempfile::TempDir;

use super::store::{job_dir, load_current, load_job_record, stage_dir};
use super::submit::{cancel, submit, Submission, SubmitRequest};
use super::{JobKind, JobRecord, JobState};

pub(super) const STAGE: &str = "stage-a";
pub(super) const HEAD_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(super) const HEAD_B: &str = "cccccccccccccccccccccccccccccccccccccccc";
pub(super) const DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

pub(super) fn state_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let work = dir.path().join(".loom").join("work");
    fs::create_dir_all(&work).unwrap();
    (dir, work)
}

pub(super) fn request<'a>(request_id: &'a str, head: &'a str, no_cache: bool) -> SubmitRequest<'a> {
    SubmitRequest {
        stage_id: STAGE,
        session_id: "session-a",
        request_id,
        kind: JobKind::Check,
        head,
        tree_digest: DIGEST,
        no_cache,
        criteria: None,
    }
}

pub(super) fn subset<'a>(request_id: &'a str, criteria: &[usize]) -> SubmitRequest<'a> {
    SubmitRequest {
        criteria: Some(criteria.to_vec()),
        ..request(request_id, HEAD_A, false)
    }
}

pub(super) fn queued(submission: Submission) -> JobRecord {
    match submission {
        Submission::Queued(record) => record,
        other => panic!("expected Submission::Queued, got {other:?}"),
    }
}

pub(super) fn joined(submission: Submission) -> JobRecord {
    match submission {
        Submission::Joined(record) => record,
        other => panic!("expected Submission::Joined, got {other:?}"),
    }
}

pub(super) fn superseded(submission: Submission) -> (JobRecord, JobRecord) {
    match submission {
        Submission::Superseded { cancelled, queued } => (*cancelled, *queued),
        other => panic!("expected Submission::Superseded, got {other:?}"),
    }
}

#[test]
fn a_first_request_queues_a_job_with_its_directory() {
    let (_dir, work) = state_root();

    let job = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());

    assert_eq!(job.state, JobState::Queued);
    assert_eq!(job.requests, ["r1"]);
    assert_eq!(job.id.len(), 32);
    assert!(job_dir(&work, STAGE, &job.id)
        .unwrap()
        .join("record.json")
        .is_file());
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(job));
}

#[test]
fn joined_requests_append_and_keep_the_last_sixteen() {
    let (_dir, work) = state_root();
    let first = queued(submit(&work, &request("r0", HEAD_A, false), Utc::now()).unwrap());
    let mut last = first.clone();
    for index in 1..20 {
        let id = format!("r{index}");
        last = joined(submit(&work, &request(&id, HEAD_A, false), Utc::now()).unwrap());
    }

    assert_eq!(last.id, first.id);
    let expected: Vec<String> = (4..20).map(|index| format!("r{index}")).collect();
    assert_eq!(last.requests, expected);
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(last));
}

#[test]
fn a_new_head_supersedes_and_cancels_the_old_job() {
    let (_dir, work) = state_root();
    let old = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());

    let (cancelled, new) =
        superseded(submit(&work, &request("r2", HEAD_B, false), Utc::now()).unwrap());

    assert_eq!(cancelled.id, old.id);
    assert_eq!(cancelled.state, JobState::Cancelled);
    let expected = format!("superseded by {}", new.id);
    assert_eq!(cancelled.detail.as_deref(), Some(expected.as_str()));
    assert_eq!(
        load_job_record(&work, STAGE, &old.id).unwrap(),
        Some(cancelled)
    );
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(new));
}

#[test]
fn an_uncached_request_waits_behind_a_cached_job() {
    let (_dir, work) = state_root();
    let cached = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());

    let (uncached, behind) =
        waiting(submit(&work, &request("r2", HEAD_A, true), Utc::now()).unwrap());

    assert_eq!(behind, cached.id);
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(cached));
    assert!(uncached.no_cache);
}

#[test]
fn a_cached_request_joins_an_uncached_job() {
    let (_dir, work) = state_root();
    let uncached = queued(submit(&work, &request("r1", HEAD_A, true), Utc::now()).unwrap());

    let job = joined(submit(&work, &request("r2", HEAD_A, false), Utc::now()).unwrap());

    assert_eq!(job.id, uncached.id);
    assert!(job.no_cache);
    assert_eq!(job.requests, ["r1", "r2"]);
}

#[test]
fn a_terminal_job_takes_a_new_request_as_a_new_job() {
    let (_dir, work) = state_root();
    let first = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());
    cancel(&work, STAGE, "operator stop").unwrap();

    let second = queued(submit(&work, &request("r2", HEAD_A, false), Utc::now()).unwrap());

    assert_ne!(second.id, first.id);
}

#[test]
fn cancel_ends_a_live_job_once_and_ignores_a_terminal_or_missing_one() {
    let (_dir, work) = state_root();
    assert_eq!(cancel(&work, STAGE, "takedown").unwrap(), None);
    assert!(
        !stage_dir(&work, STAGE).unwrap().exists(),
        "a cancel with no job must not create the stage directory"
    );
    let job = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());

    let cancelled = cancel(&work, STAGE, "takedown")
        .unwrap()
        .expect("a cancelled job");

    assert_eq!(cancelled.id, job.id);
    assert_eq!(cancelled.state, JobState::Cancelled);
    assert_eq!(cancelled.detail.as_deref(), Some("takedown"));
    assert!(cancelled.finished_at.is_some());
    assert_eq!(cancel(&work, STAGE, "again").unwrap(), None);
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(cancelled));
}

#[test]
fn a_malformed_head_or_tree_digest_is_refused_and_queues_nothing() {
    let (_dir, work) = state_root();
    let now = Utc::now();
    let upper = HEAD_A.to_uppercase();
    for head in ["aaaa", upper.as_str(), ""] {
        let refused = submit(&work, &request("r1", head, false), now).unwrap();
        assert!(
            matches!(&refused, Submission::Refused(why) if why.contains("head")),
            "{refused:?}"
        );
    }
    let mut bad_digest = request("r2", HEAD_A, false);
    bad_digest.tree_digest = "bbbb";
    let refused = submit(&work, &bad_digest, now).unwrap();
    assert!(
        matches!(&refused, Submission::Refused(why) if why.contains("tree_digest")),
        "{refused:?}"
    );
    assert_eq!(load_current(&work, STAGE).unwrap(), None);
}

#[test]
fn a_subset_request_joins_a_running_full_check() {
    let (_dir, work) = state_root();
    let full = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());

    let job = joined(submit(&work, &subset("r2", &[1, 3]), Utc::now()).unwrap());

    assert_eq!(job.id, full.id);
    assert_eq!(job.criteria, None);
}

#[test]
fn a_full_request_waits_behind_a_subset_job() {
    let (_dir, work) = state_root();
    let partial = queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    assert_eq!(partial.criteria, Some(vec![1]));

    let (full, behind) = waiting(submit(&work, &request("r2", HEAD_A, false), Utc::now()).unwrap());

    assert_eq!(behind, partial.id);
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(partial));
    assert_eq!(full.criteria, None);
}

#[test]
fn equal_subsets_join_and_a_wider_subset_waits() {
    let (_dir, work) = state_root();
    let first = queued(submit(&work, &subset("r1", &[0, 2]), Utc::now()).unwrap());

    let same = joined(submit(&work, &subset("r2", &[2, 0]), Utc::now()).unwrap());
    let (_, behind) = waiting(submit(&work, &subset("r3", &[0, 1]), Utc::now()).unwrap());

    assert_eq!(same.id, first.id);
    assert_eq!(behind, first.id);
}

#[test]
fn a_selection_outside_a_check_or_naming_nothing_is_refused() {
    let (_dir, work) = state_root();
    let whole_run = SubmitRequest {
        kind: JobKind::Complete,
        ..subset("r1", &[0])
    };
    let empty = subset("r2", &[]);

    for request in [whole_run, empty] {
        let refused = submit(&work, &request, Utc::now()).unwrap();
        assert!(matches!(refused, Submission::Refused(_)), "{refused:?}");
    }
    assert_eq!(load_current(&work, STAGE).unwrap(), None);
}

pub(super) fn waiting(submission: Submission) -> (JobRecord, String) {
    match submission {
        Submission::Waiting { pending, behind } => (*pending, behind),
        other => panic!("expected Submission::Waiting, got {other:?}"),
    }
}
