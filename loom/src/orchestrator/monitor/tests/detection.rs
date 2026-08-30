//! Stage/session change detection and the context-band events.

use crate::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{MonitorConfig, MonitorEvent};

#[test]
fn test_detect_stage_completion() {
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;

    // First poll - stage appears as Executing (no previous state, no event)
    let events = detection.detect_stage_changes(&[stage.clone()]);
    assert_eq!(events.len(), 0);

    // Stage completes - should generate StageCompleted event
    stage.status = StageStatus::Completed;
    let events = detection.detect_stage_changes(&[stage]);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::StageCompleted { stage_id } = &events[0] {
        assert_eq!(stage_id, "stage-1");
    } else {
        panic!("Expected StageCompleted event");
    }
}

#[test]
fn test_detect_session_crash() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Spawning;

    let events = detection.detect_session_changes(&[session.clone()], &[], &handlers);
    assert_eq!(events.len(), 0);

    session.status = SessionStatus::Crashed;
    let events = detection.detect_session_changes(&[session], &[], &handlers);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::SessionCrashed {
        session_id,
        stage_id,
        crash_report_path: _,
    } = &events[0]
    {
        assert_eq!(session_id, "session-1");
        assert_eq!(stage_id, &None);
    } else {
        panic!("Expected SessionCrashed event");
    }
}

#[test]
fn test_detect_context_warning() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.context_tokens = 50_000; // a third of the 150k default - Green

    detection.detect_session_changes(&[session.clone()], &[], &handlers);

    session.context_tokens = 100_000; // 67% of the default ceiling - Yellow
    let events = detection.detect_session_changes(&[session], &[], &handlers);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::SessionContextWarning {
        session_id,
        context_tokens,
        ceiling_tokens,
    } = &events[0]
    {
        assert_eq!(session_id, "session-1");
        assert_eq!(*context_tokens, 100_000);
        assert_eq!(*ceiling_tokens, DEFAULT_CONTEXT_CEILING_TOKENS);
    } else {
        panic!("Expected SessionContextWarning event");
    }
}

#[test]
fn test_detect_context_critical() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.context_tokens = 50_000; // a third of the 150k default - Green

    detection.detect_session_changes(&[session.clone()], &[], &handlers);

    session.context_tokens = 140_000; // 93% of the default ceiling - Red
    let events = detection.detect_session_changes(&[session], &[], &handlers);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::SessionContextCritical {
        session_id,
        context_tokens,
        ceiling_tokens,
    } = &events[0]
    {
        assert_eq!(session_id, "session-1");
        assert_eq!(*context_tokens, 140_000);
        assert_eq!(*ceiling_tokens, DEFAULT_CONTEXT_CEILING_TOKENS);
    } else {
        panic!("Expected SessionContextCritical event");
    }
}

#[test]
fn test_stage_blocked_event() {
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;

    detection.detect_stage_changes(&[stage.clone()]);

    stage.status = StageStatus::Blocked;
    stage.close_reason = Some("Dependency failed".to_string());

    let events = detection.detect_stage_changes(&[stage]);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::StageBlocked { stage_id, reason } = &events[0] {
        assert_eq!(stage_id, "stage-1");
        assert_eq!(reason, "Dependency failed");
    } else {
        panic!("Expected StageBlocked event");
    }
}

#[test]
fn test_session_needs_handoff_event() {
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some("session-1".to_string());

    detection.detect_stage_changes(&[stage.clone()]);

    stage.status = StageStatus::NeedsHandoff;

    let events = detection.detect_stage_changes(&[stage.clone()]);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::SessionNeedsHandoff {
        session_id,
        stage_id,
    } = &events[0]
    {
        assert_eq!(session_id, "session-1");
        assert_eq!(stage_id, "stage-1");
    } else {
        panic!("Expected SessionNeedsHandoff event");
    }

    // Handoff remains level-triggered until the handler proves takedown and
    // re-queues the stage. A transient fail-closed error must not latch it.
    let retry_events = detection.detect_stage_changes(&[stage]);
    assert_eq!(retry_events.len(), 1);
    assert!(matches!(
        &retry_events[0],
        MonitorEvent::SessionNeedsHandoff { session_id, stage_id }
            if session_id == "session-1" && stage_id == "stage-1"
    ));
}

#[test]
fn test_merge_session_completed_event() {
    // Test that MergeSessionCompleted event can be created and compared
    let event1 = MonitorEvent::MergeSessionCompleted {
        session_id: "session-1".to_string(),
        stage_id: "stage-1".to_string(),
    };
    let event2 = MonitorEvent::MergeSessionCompleted {
        session_id: "session-1".to_string(),
        stage_id: "stage-1".to_string(),
    };
    let event3 = MonitorEvent::MergeSessionCompleted {
        session_id: "session-2".to_string(),
        stage_id: "stage-1".to_string(),
    };

    assert_eq!(event1, event2);
    assert_ne!(event1, event3);
}

#[test]
fn test_check_session_alive_without_liveness_returns_none() {
    use tempfile::TempDir;

    // Liveness is now backend-aware (LivenessService) rather than
    // peeking at the host PID file directly. A Handlers built without
    // a LivenessService deliberately returns None so the detection
    // loop skips crash reporting for that tick instead of guessing.
    let temp_dir = TempDir::new().unwrap();
    let config = MonitorConfig {
        work_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);

    let mut session = Session::new();
    session.assign_to_stage("test-stage".to_string());
    session.set_pid(99999);

    let result = handlers.check_session_alive(&session).unwrap();
    assert_eq!(result, None);
}
