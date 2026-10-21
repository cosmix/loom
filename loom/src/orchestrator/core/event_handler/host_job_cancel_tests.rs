//! A stage takedown cancels the stage's host job: driven through the takedown
//! helper itself, and through a hung-session recovery, which reaches it by
//! `begin_handoff` and the handoff's takedown as every handoff path does.

use std::path::Path;

use chrono::Utc;

use super::recover_hung_tests::{report, stalled_stage, ESCALATING_SILENCE_SECS};
use super::tests::{executing_stage, handoff_work_dir, orchestrator_for};
use crate::host_verify::store::load_current;
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::{JobKind, JobRecord, JobState};
use crate::models::session::SessionExitReason;

const STAGE: &str = "test-stage";

fn queue_check(work: &Path) -> JobRecord {
    let request = SubmitRequest {
        stage_id: STAGE,
        session_id: "session-a",
        request_id: "r1",
        kind: JobKind::Check,
        head: "aaaa",
        tree_digest: "bbbb",
        no_cache: false,
    };
    match submit(work, &request, Utc::now()).unwrap() {
        Submission::Queued(record) => record,
        other => panic!("expected a queued job, got {other:?}"),
    }
}

fn assert_cancelled_by_takedown(work: &Path, job: &JobRecord) {
    let current = load_current(work, STAGE)
        .unwrap()
        .expect("a current record");
    assert_eq!(current.id, job.id);
    assert_eq!(current.state, JobState::Cancelled);
    assert_eq!(current.detail.as_deref(), Some("stage agents taken down"));
}

#[test]
fn taking_down_a_stages_agents_cancels_its_host_job() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    let job = queue_check(&work);
    let mut orchestrator = orchestrator_for(&work, temp.path());

    let survivors = orchestrator
        .take_down_agents(STAGE, Vec::new(), SessionExitReason::Replaced)
        .unwrap();

    assert!(survivors.is_empty());
    assert_cancelled_by_takedown(&work, &job);
}

#[test]
fn recovering_a_hung_session_cancels_its_stages_host_job() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = stalled_stage(&work);
    let job = queue_check(&work);
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing(STAGE).unwrap();
    orchestrator
        .active_sessions
        .insert(STAGE.to_string(), session.clone());

    orchestrator
        .on_session_hung(report(&session.id, ESCALATING_SILENCE_SECS))
        .unwrap();

    assert!(
        !crate::process::is_process_alive(agent_pid),
        "the hung agent must be taken down"
    );
    assert_cancelled_by_takedown(&work, &job);
}
