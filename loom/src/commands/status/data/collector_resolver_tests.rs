//! Which session speaks for a stage in a merge state, and what the resolver's
//! activity reads as. `stage.session` keeps naming the worker that finished.

use super::tests::{make_test_stage, temp_work_dir};
use super::*;
use crate::commands::status::data::ActivityStatus;
use crate::models::constants::STALENESS_THRESHOLD_SECS;
use crate::models::session::{SessionBackendKind, SessionStatus};
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat};

/// A merge stage whose worker `worker-1` finished (`Completed`, pid gone) and
/// is still the one `stage.session` names.
fn merging_stage_with_ended_worker(status: StageStatus) -> (Stage, Session) {
    let mut stage = make_test_stage("stage-1", status);
    stage.session = Some("worker-1".to_string());
    let mut worker = Session::new();
    worker.id = "worker-1".to_string();
    worker.stage_id = Some("stage-1".to_string());
    worker.status = SessionStatus::Completed;
    worker.pid = Some(999_999_999);
    worker.backend = SessionBackendKind::Tmux;
    (stage, worker)
}

/// A running tmux resolver for `stage-1` whose process is this test's own.
fn live_resolver() -> Session {
    let mut resolver = Session::new();
    resolver.id = "resolver-1".to_string();
    resolver.session_type = SessionType::Merge;
    resolver.stage_id = Some("stage-1".to_string());
    resolver.status = SessionStatus::Running;
    resolver.pid = Some(std::process::id());
    resolver.backend = SessionBackendKind::Tmux;
    resolver
}

#[test]
fn a_live_resolver_speaks_for_a_merge_stage_whose_named_worker_ended() {
    let (_tmp, work_dir) = temp_work_dir();
    for status in [StageStatus::MergeConflict, StageStatus::MergeBlocked] {
        let (stage, worker) = merging_stage_with_ended_worker(status);

        let summary = build_stage_summary(&stage, &[worker, live_resolver()], &work_dir);

        assert_eq!(summary.pid, Some(std::process::id()));
        assert!(summary.session_alive);
        assert_eq!(summary.session_backend, Some(SessionBackendKind::Tmux));
        assert_eq!(summary.session_type, Some(SessionType::Merge));
        assert_eq!(
            summary.merge_resolver_session.as_deref(),
            Some("resolver-1")
        );
    }
}

#[test]
fn the_newest_live_resolver_speaks_whatever_the_record_order() {
    let (_tmp, work_dir) = temp_work_dir();
    let (stage, worker) = merging_stage_with_ended_worker(StageStatus::MergeConflict);
    let mut older = live_resolver();
    older.id = "resolver-old".to_string();
    older.created_at -= chrono::Duration::minutes(10);
    let newer = live_resolver();

    for sessions in [
        vec![worker.clone(), older.clone(), newer.clone()],
        vec![worker, newer, older],
    ] {
        let summary = build_stage_summary(&stage, &sessions, &work_dir);

        assert_eq!(
            summary.merge_resolver_session.as_deref(),
            Some("resolver-1")
        );
    }
}

#[test]
fn the_ended_worker_still_speaks_when_no_resolver_is_live() {
    let (_tmp, work_dir) = temp_work_dir();
    let (stage, worker) = merging_stage_with_ended_worker(StageStatus::MergeConflict);
    let mut gone = live_resolver();
    gone.pid = Some(999_999_999);

    let summary = build_stage_summary(&stage, &[worker, gone], &work_dir);

    assert!(!summary.session_alive);
    assert_eq!(summary.session_type, Some(SessionType::Stage));
    assert_eq!(summary.merge_resolver_session, None);
}

#[test]
fn a_live_resolver_does_not_replace_the_worker_outside_a_merge_state() {
    let (_tmp, work_dir) = temp_work_dir();
    let (stage, worker) = merging_stage_with_ended_worker(StageStatus::Completed);

    let summary = build_stage_summary(&stage, &[worker, live_resolver()], &work_dir);

    assert!(!summary.session_alive);
    assert_eq!(summary.session_type, Some(SessionType::Stage));
    assert_eq!(summary.merge_resolver_session, None);
}

#[test]
fn a_resolver_is_measured_from_its_spawn_until_it_makes_a_tool_call() {
    let (_tmp, work_dir) = temp_work_dir();
    let (stage, worker) = merging_stage_with_ended_worker(StageStatus::MergeConflict);
    let silence = STALENESS_THRESHOLD_SECS + 60;
    let mut resolver = live_resolver();
    resolver.created_at = Utc::now() - chrono::Duration::seconds(i64::try_from(silence).unwrap());
    // The worker's heartbeat is fresh, and says nothing of the resolver.
    let mut worker_beat = Heartbeat::new(stage.id.clone(), worker.id.clone());
    worker_beat.last_tool = Some("Bash".to_string());
    write_heartbeat(work_dir.root(), &worker_beat).unwrap();
    let sessions = [worker, resolver.clone()];

    let idle = build_stage_summary(&stage, &sessions, &work_dir);

    assert_eq!(idle.activity_status, ActivityStatus::Stale);
    assert!(idle.staleness_secs.is_some_and(|secs| secs >= silence));
    assert_eq!(idle.last_tool, None);

    write_heartbeat(
        work_dir.root(),
        &Heartbeat::new(stage.id.clone(), resolver.id.clone()),
    )
    .unwrap();

    let working = build_stage_summary(&stage, &sessions, &work_dir);

    assert_eq!(working.activity_status, ActivityStatus::Working);
    assert!(working.staleness_secs.is_some_and(|secs| secs < 60));
}
