//! Context-ceiling retry, restart, and fresh-heartbeat regressions.

use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat};
use crate::orchestrator::monitor::{Monitor, MonitorConfig, MonitorEvent};
use crate::verify::transitions::create_stage;

fn ceiling_harness() -> (tempfile::TempDir, Handlers) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = MonitorConfig {
        work_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    (temp_dir, Handlers::new(config, None))
}

pub(super) fn budget_retry_pair() -> (Session, Stage) {
    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("stage-1".to_string());
    session.context_tokens = 130_000;

    let mut stage = Stage::new("test".to_string(), Some("Ceiling test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::NeedsHandoff;
    stage.session = Some(session.id.clone());
    stage.context_ceiling_tokens = Some(100_000);
    (session, stage)
}

fn handoff_count(work_dir: &std::path::Path) -> usize {
    std::fs::read_dir(work_dir.join("handoffs"))
        .unwrap()
        .count()
}

/// Persisted `Running` records also outlive their assignments. On restart an
/// old predecessor must not create a Red handoff against its successor's
/// stage merely because both records still name the same stage id.
#[test]
fn a_running_predecessor_is_not_context_judged_for_its_successors_stage() {
    let (temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();
    let mut stage = Stage::new("test".to_string(), None);
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some("session-new".to_string());
    stage.context_ceiling_tokens = Some(100_000);

    let mut predecessor = Session::new();
    predecessor.id = "session-old".to_string();
    predecessor.status = SessionStatus::Running;
    predecessor.stage_id = Some(stage.id.clone());
    predecessor.context_tokens = 200_000;

    let events = detection.detect_session_changes(&[predecessor], &[stage], &handlers);

    assert!(!events.iter().any(|event| matches!(
        event,
        MonitorEvent::SessionContextCritical { .. } | MonitorEvent::BudgetExceeded { .. }
    )));
    assert!(!temp_dir.path().join("handoffs").exists());
}

#[test]
fn a_new_red_crossing_gets_a_fresh_snapshot_but_a_restart_does_not() {
    let (temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();
    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("stage-1".to_string());
    let mut stage = Stage::new("test".to_string(), None);
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    stage.context_ceiling_tokens = Some(100_000);

    for tokens in [95_000, 50_000, 96_000] {
        session.context_tokens = tokens;
        detection.detect_session_changes(
            std::slice::from_ref(&session),
            std::slice::from_ref(&stage),
            &handlers,
        );
    }
    assert_eq!(handoff_count(temp_dir.path()), 2);

    Detection::new().detect_session_changes(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &handlers,
    );
    assert_eq!(
        handoff_count(temp_dir.path()),
        2,
        "a cold-start observation should reuse the latest durable Red crossing"
    );

    session.context_tokens = 110_000;
    Detection::new().detect_session_changes(&[session], &[stage], &handlers);
    assert_eq!(
        handoff_count(temp_dir.path()),
        3,
        "a cold start with newer resident context must refresh the Red snapshot"
    );
}

#[test]
fn an_unchanged_red_session_retries_a_failed_handoff_write() {
    let (temp_dir, handlers) = ceiling_harness();
    std::fs::write(
        temp_dir.path().join("handoffs"),
        "blocks directory creation",
    )
    .unwrap();
    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("stage-1".to_string());
    session.context_tokens = 95_000;
    let mut stage = Stage::new("test".to_string(), None);
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    stage.context_ceiling_tokens = Some(100_000);
    let mut detection = Detection::new();

    detection.detect_session_changes(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &handlers,
    );
    assert!(!detection.red_handoff_ready.contains(&session.id));

    std::fs::remove_file(temp_dir.path().join("handoffs")).unwrap();
    detection.detect_session_changes(&[session.clone()], &[stage], &handlers);
    assert!(detection.red_handoff_ready.contains(&session.id));
    assert_eq!(handoff_count(temp_dir.path()), 1);
}

/// A failed backstop takedown leaves the stage in `NeedsHandoff`. Its next
/// poll must retry the budget event (not silently turn into a generic handoff)
/// while the same live session is still over the ceiling.
#[test]
fn budget_exceeded_retries_while_its_matching_stage_needs_handoff() {
    let (_temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();
    let (session, stage) = budget_retry_pair();

    let first = detection.detect_session_changes(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &handlers,
    );
    assert!(first
        .iter()
        .any(|event| matches!(event, MonitorEvent::BudgetExceeded { .. })));

    let retry = detection.detect_session_changes(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &handlers,
    );
    assert!(
        retry
            .iter()
            .any(|event| matches!(event, MonitorEvent::BudgetExceeded { .. })),
        "a matching NeedsHandoff stage must re-emit the budget-owned retry: {retry:?}"
    );

    assert!(
        detection.detect_stage_changes(&[stage]).is_empty(),
        "the generic handoff event must not race the budget retry"
    );
}

/// A liveness check can make a still-`Running` snapshot terminal within this
/// poll. That transition must clear its latch before stage retry detection.
#[test]
fn a_terminal_budget_observation_releases_the_generic_handoff_retry() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let handlers = Handlers::new(
        MonitorConfig {
            work_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        },
        Some(crate::orchestrator::liveness::LivenessService::fixed_for_tests(false)),
    );
    let mut detection = Detection::new();
    let (session, stage) = budget_retry_pair();

    detection.detect_session_changes(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &handlers,
    );
    detection.detect_session_changes(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &handlers,
    );

    assert!(!detection.last_budget_exceeded.contains_key("session-1"));
    assert!(matches!(
        detection.detect_stage_changes(&[stage]).as_slice(),
        [MonitorEvent::SessionNeedsHandoff { session_id, stage_id }]
            if session_id == "session-1" && stage_id == "stage-1"
    ));
}

#[test]
fn a_missing_session_drops_its_stale_budget_latch() {
    let (_temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();
    detection
        .last_budget_exceeded
        .insert("old-session".to_string(), true);

    detection.detect_session_changes(&[], &[], &handlers);

    assert!(detection.last_budget_exceeded.is_empty());
}

/// Poll computes session state first so its budget latch can steer stage
/// retries, but still returns stage events before session events to preserve
/// the monitor's established event-consumer contract.
#[test]
fn poll_preserves_stage_before_session_event_order_after_latching_budget() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let mut blocked = Stage::new("blocked".to_string(), None);
    blocked.id = "blocked".to_string();
    blocked.status = StageStatus::Blocked;
    blocked.close_reason = Some("test ordering".to_string());
    create_stage(&blocked, &work_dir).unwrap();

    let (session, handoff_stage) = budget_retry_pair();
    create_stage(&handoff_stage, &work_dir).unwrap();
    save_session(&session, &work_dir).unwrap();

    let mut monitor = Monitor::new(MonitorConfig {
        work_dir,
        ..Default::default()
    });
    let events = monitor.poll().unwrap();

    assert!(matches!(
        events.first(),
        Some(MonitorEvent::StageBlocked { .. })
    ));
    assert!(matches!(
        events.last(),
        Some(MonitorEvent::BudgetExceeded { .. })
    ));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, MonitorEvent::SessionNeedsHandoff { .. })),
        "the budget latch must suppress the generic retry in the same poll: {events:?}"
    );
}

/// Resident context is not monotonic: native compaction can lower it. A fresh
/// low heartbeat in the same poll must replace a stale high session record
/// before the daemon decides whether to fire its destructive backstop.
#[test]
fn poll_judges_the_fresh_heartbeat_instead_of_a_stale_high_session_record() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let (mut session, mut stage) = budget_retry_pair();
    stage.status = StageStatus::Executing;
    session.context_tokens = 130_000;
    create_stage(&stage, &work_dir).unwrap();
    save_session(&session, &work_dir).unwrap();
    write_heartbeat(
        &work_dir,
        &Heartbeat::new(stage.id.clone(), session.id.clone()).with_context_tokens(80_000),
    )
    .unwrap();

    let events = Monitor::new(MonitorConfig {
        work_dir,
        ..Default::default()
    })
    .poll()
    .unwrap();

    assert!(events.iter().any(|event| matches!(
        event,
        MonitorEvent::HeartbeatReceived {
            context_tokens: Some(80_000),
            ..
        }
    )));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, MonitorEvent::BudgetExceeded { .. })),
        "a stale persisted high reading must not beat a fresh low heartbeat: {events:?}"
    );
}
