//! A hold appearing after detection must not consume the later escalation.

use super::*;
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::{store, JobKind, JobRecord, JobState};
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::monitor::MonitorEvent;

fn queue_job(work: &Path, session: &Session) -> JobRecord {
    let request = SubmitRequest {
        stage_id: "test-stage",
        session_id: &session.id,
        request_id: "latch-race",
        kind: JobKind::Check,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: None,
    };
    match submit(work, &request, chrono::Utc::now()).unwrap() {
        Submission::Queued(job) => job,
        other => panic!("expected queued job, got {other:?}"),
    }
}

fn poll_hung(orchestrator: &mut Orchestrator) -> bool {
    orchestrator
        .monitor
        .poll()
        .unwrap()
        .iter()
        .any(|event| matches!(event, MonitorEvent::SessionHung { .. }))
}

#[test]
fn handler_hold_after_detection_rearms_the_event_without_a_new_heartbeat() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom/work");
    executing_stage(&work);
    let session = recorded_session(&work);
    assign_stage_session(&work, &session.id);
    let mut heartbeat = Heartbeat::new("test-stage".into(), session.id.clone());
    heartbeat.timestamp = chrono::Utc::now() - chrono::Duration::seconds(901);
    heartbeat.progress_at = Some(heartbeat.timestamp);
    write_heartbeat(&work, &heartbeat).unwrap();
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .monitor
        .set_liveness(LivenessService::fixed_for_tests(true));
    assert!(poll_hung(&mut orchestrator));
    assert!(!poll_hung(&mut orchestrator));

    let mut job = queue_job(&work, &session);
    orchestrator
        .on_session_hung(report(&session.id, 901))
        .unwrap();
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Executing
    );
    assert!(!poll_hung(&mut orchestrator));

    job.state = JobState::Passed;
    store::save_current(&work, &job).unwrap();
    assert!(poll_hung(&mut orchestrator));
    assert!(!poll_hung(&mut orchestrator));
}
