//! Merge-session recognition and the crashes it must not report.

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{MonitorConfig, MonitorEvent};

#[test]
fn test_is_merge_session_with_merge_signal() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();
    let signals_dir = work_dir.join("signals");
    std::fs::create_dir_all(&signals_dir).unwrap();

    // Create a merge signal file
    let merge_signal_content = r#"# Merge Signal: session-merge-123

## Merge Context

You are resolving a **merge conflict** in the main repository.

## Target

- **Session**: session-merge-123
- **Stage**: stage-1
- **Source Branch**: loom/stage-1
- **Target Branch**: main

## Conflicting Files

- `src/main.rs`
"#;
    std::fs::write(
        signals_dir.join("session-merge-123.md"),
        merge_signal_content,
    )
    .unwrap();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);

    assert!(handlers.is_merge_session("session-merge-123"));
    assert!(!handlers.is_merge_session("nonexistent-session"));
}

#[test]
fn test_is_merge_session_with_regular_signal() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();
    let signals_dir = work_dir.join("signals");
    std::fs::create_dir_all(&signals_dir).unwrap();

    // Create a regular (non-merge) signal file
    let regular_signal_content = r#"# Signal: session-regular-123

## Worktree Context

You are in an **isolated git worktree**.

## Target

- **Session**: session-regular-123
- **Stage**: stage-1
"#;
    std::fs::write(
        signals_dir.join("session-regular-123.md"),
        regular_signal_content,
    )
    .unwrap();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);

    // Regular signal should not be detected as a merge session
    assert!(!handlers.is_merge_session("session-regular-123"));
}

#[test]
fn test_merge_conflict_stage_session_not_reported_as_crash() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    let handlers = Handlers::new(
        config,
        Some(crate::orchestrator::liveness::LivenessService::fixed_for_tests(false)),
    );
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.stage_id = Some("stage-1".to_string());
    session.status = SessionStatus::Running;
    session.set_pid(99999); // Non-existent PID

    let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::MergeConflict;

    // First poll establishes Running state in detection tracking
    detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);

    // Second poll: PID dead + stage is MergeConflict → treat as normal exit, not crash
    let events = detection.detect_session_changes(&[session.clone()], &[stage], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionCrashed { .. })),
        "MergeConflict stage should prevent crash report when session PID dies"
    );
    assert_eq!(
        detection.last_session_states.get("session-1"),
        Some(&SessionStatus::Completed),
        "Session should be marked Completed, not Crashed"
    );
}

#[test]
fn test_merge_blocked_stage_session_not_reported_as_crash() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    let handlers = Handlers::new(
        config,
        Some(crate::orchestrator::liveness::LivenessService::fixed_for_tests(false)),
    );
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-2".to_string();
    session.stage_id = Some("stage-2".to_string());
    session.status = SessionStatus::Running;
    session.set_pid(99998); // Non-existent PID

    let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
    stage.id = "stage-2".to_string();
    stage.status = StageStatus::MergeBlocked;

    // First poll establishes Running state
    detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);

    // Second poll: PID dead + stage is MergeBlocked → normal exit, not crash
    let events = detection.detect_session_changes(&[session.clone()], &[stage], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionCrashed { .. })),
        "MergeBlocked stage should prevent crash report when session PID dies"
    );
    assert_eq!(
        detection.last_session_states.get("session-2"),
        Some(&SessionStatus::Completed),
        "Session should be marked Completed, not Crashed"
    );
}
