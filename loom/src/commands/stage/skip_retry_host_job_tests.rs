//! `loom stage retry` cancels the stage's host job once the requeue stands.

use chrono::Utc;
use tempfile::TempDir;

use super::persist_retry_delta;
use crate::host_verify::store::load_current;
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::{JobKind, JobState};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{create_stage, load_stage};

#[test]
fn retry_cancels_the_stages_host_job() {
    let temp = TempDir::new().unwrap();
    let mut stage = Stage::new("s1".to_string(), None);
    stage.id = "s1".to_string();
    stage.status = StageStatus::Blocked;
    create_stage(&stage, temp.path()).unwrap();
    let request = SubmitRequest {
        stage_id: "s1",
        session_id: "session-a",
        request_id: "r1",
        kind: JobKind::Check,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: None,
    };
    let Submission::Queued(job) = submit(temp.path(), &request, Utc::now()).unwrap() else {
        panic!("expected a queued job");
    };

    persist_retry_delta("s1", temp.path(), &stage, &StageStatus::Blocked, false).unwrap();

    let stage = load_stage("s1", temp.path()).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    let current = load_current(temp.path(), "s1")
        .unwrap()
        .expect("a current record");
    assert_eq!(current.id, job.id);
    assert_eq!(current.state, JobState::Cancelled);
    assert_eq!(current.detail.as_deref(), Some("stage retried"));
}
