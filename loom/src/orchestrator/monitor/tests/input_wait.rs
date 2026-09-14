//! Tests for reconciling a stage stuck in `WaitingForInput` whose own
//! session kept making tool progress after the transition.

use chrono::{DateTime, Duration, Utc};

use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat, HeartbeatWatcher};
use crate::orchestrator::monitor::input_wait::reconcile_stale_input_waits;
use crate::verify::transitions::{load_stage, save_stage};

fn waiting_stage(id: &str, session_id: &str, updated_at: DateTime<Utc>) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Stage {id}"),
        status: StageStatus::WaitingForInput,
        session: Some(session_id.to_string()),
        updated_at,
        ..Stage::default()
    }
}

fn executing_stage(id: &str, session_id: &str, updated_at: DateTime<Utc>) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Stage {id}"),
        status: StageStatus::Executing,
        session: Some(session_id.to_string()),
        updated_at,
        ..Stage::default()
    }
}

fn heartbeat_with_progress(
    stage_id: &str,
    session_id: &str,
    progress_at: DateTime<Utc>,
    last_tool: Option<&str>,
) -> Heartbeat {
    let mut heartbeat = Heartbeat::new(stage_id.to_string(), session_id.to_string());
    heartbeat.timestamp = progress_at;
    heartbeat.progress_at = Some(progress_at);
    heartbeat.last_tool = last_tool.map(str::to_string);
    heartbeat.subagent = false;
    heartbeat
}

fn watcher_after_poll(work_dir: &std::path::Path) -> HeartbeatWatcher {
    let mut watcher = HeartbeatWatcher::new();
    watcher.poll(work_dir).unwrap();
    watcher
}

#[test]
fn resumes_a_stage_whose_session_progressed_after_the_wait_began() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let now = Utc::now();
    let updated_at = now - Duration::seconds(60);
    let stage = waiting_stage("stage-1", "session-1", updated_at);
    save_stage(&stage, work_dir).unwrap();
    write_heartbeat(
        work_dir,
        &heartbeat_with_progress(
            "stage-1",
            "session-1",
            updated_at + Duration::seconds(30),
            Some("Bash"),
        ),
    )
    .unwrap();
    let watcher = watcher_after_poll(work_dir);

    let resumed = reconcile_stale_input_waits(work_dir, &[stage], &watcher);

    assert_eq!(resumed, vec!["stage-1".to_string()]);
    assert_eq!(
        load_stage("stage-1", work_dir).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn leaves_a_stage_alone_when_progress_predates_the_wait() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let now = Utc::now();
    let updated_at = now - Duration::seconds(60);
    let stage = waiting_stage("stage-1", "session-1", updated_at);
    save_stage(&stage, work_dir).unwrap();
    write_heartbeat(
        work_dir,
        &heartbeat_with_progress(
            "stage-1",
            "session-1",
            updated_at - Duration::seconds(30),
            Some("Bash"),
        ),
    )
    .unwrap();
    let watcher = watcher_after_poll(work_dir);

    let resumed = reconcile_stale_input_waits(work_dir, &[stage], &watcher);

    assert!(resumed.is_empty());
    assert_eq!(
        load_stage("stage-1", work_dir).unwrap().status,
        StageStatus::WaitingForInput
    );
}

#[test]
fn leaves_a_stage_alone_when_the_heartbeat_belongs_to_a_previous_session() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let now = Utc::now();
    let updated_at = now - Duration::seconds(60);
    let stage = waiting_stage("stage-1", "session-1", updated_at);
    save_stage(&stage, work_dir).unwrap();
    write_heartbeat(
        work_dir,
        &heartbeat_with_progress(
            "stage-1",
            "session-0",
            updated_at + Duration::seconds(30),
            Some("Bash"),
        ),
    )
    .unwrap();
    let watcher = watcher_after_poll(work_dir);

    let resumed = reconcile_stale_input_waits(work_dir, &[stage], &watcher);

    assert!(resumed.is_empty());
    assert_eq!(
        load_stage("stage-1", work_dir).unwrap().status,
        StageStatus::WaitingForInput
    );
}

#[test]
fn leaves_a_stage_alone_when_the_heartbeat_is_the_answered_question_itself() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let now = Utc::now();
    let updated_at = now - Duration::seconds(60);
    let stage = waiting_stage("stage-1", "session-1", updated_at);
    save_stage(&stage, work_dir).unwrap();
    write_heartbeat(
        work_dir,
        &heartbeat_with_progress(
            "stage-1",
            "session-1",
            updated_at + Duration::seconds(30),
            Some("AskUserQuestion"),
        ),
    )
    .unwrap();
    let watcher = watcher_after_poll(work_dir);

    let resumed = reconcile_stale_input_waits(work_dir, &[stage], &watcher);

    assert!(resumed.is_empty());
    assert_eq!(
        load_stage("stage-1", work_dir).unwrap().status,
        StageStatus::WaitingForInput
    );
}

#[test]
fn leaves_a_stage_alone_when_the_progress_came_from_a_subagent() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let now = Utc::now();
    let updated_at = now - Duration::seconds(60);
    let stage = waiting_stage("stage-1", "session-1", updated_at);
    save_stage(&stage, work_dir).unwrap();
    let mut heartbeat = heartbeat_with_progress(
        "stage-1",
        "session-1",
        updated_at + Duration::seconds(30),
        Some("Bash"),
    );
    heartbeat.subagent = true;
    write_heartbeat(work_dir, &heartbeat).unwrap();
    let watcher = watcher_after_poll(work_dir);

    let resumed = reconcile_stale_input_waits(work_dir, &[stage], &watcher);

    assert!(resumed.is_empty());
    assert_eq!(
        load_stage("stage-1", work_dir).unwrap().status,
        StageStatus::WaitingForInput
    );
}

#[test]
fn leaves_an_executing_stage_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let now = Utc::now();
    let updated_at = now - Duration::seconds(60);
    let stage = executing_stage("stage-1", "session-1", updated_at);
    save_stage(&stage, work_dir).unwrap();
    write_heartbeat(
        work_dir,
        &heartbeat_with_progress("stage-1", "session-1", now, Some("Bash")),
    )
    .unwrap();
    let watcher = watcher_after_poll(work_dir);

    let resumed = reconcile_stale_input_waits(work_dir, &[stage], &watcher);

    assert!(resumed.is_empty());
    assert_eq!(
        load_stage("stage-1", work_dir).unwrap().status,
        StageStatus::Executing
    );
}
