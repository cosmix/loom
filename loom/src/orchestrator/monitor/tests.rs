//! Tests for the monitor module

use std::path::PathBuf;
use std::time::Duration;

use crate::models::constants::{
    CONTEXT_CRITICAL_THRESHOLD, CONTEXT_WARNING_THRESHOLD, DEFAULT_CONTEXT_LIMIT,
};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::core::parse_session_from_markdown;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{
    context_health, context_usage_percent, ContextHealth, MonitorConfig, MonitorEvent,
};
use crate::verify::transitions::parse_stage_from_markdown;

#[test]
fn test_monitor_config_default() {
    let config = MonitorConfig::default();
    assert_eq!(config.poll_interval, Duration::from_secs(5));
    assert_eq!(config.work_dir, PathBuf::from(".work"));
    assert_eq!(config.context_warning_threshold, CONTEXT_WARNING_THRESHOLD);
    assert_eq!(
        config.context_critical_threshold,
        CONTEXT_CRITICAL_THRESHOLD
    );
}

#[test]
fn test_context_health_green() {
    let tokens = 50_000;
    let limit = DEFAULT_CONTEXT_LIMIT;
    let health = context_health(tokens, limit);
    assert_eq!(health, ContextHealth::Green);
}

#[test]
fn test_context_health_yellow() {
    // 55% - in the warning zone (50-64%)
    let tokens = 110_000;
    let limit = DEFAULT_CONTEXT_LIMIT;
    let health = context_health(tokens, limit);
    assert_eq!(health, ContextHealth::Yellow);
}

#[test]
fn test_context_health_red() {
    // 65% - at the critical threshold
    let tokens = 130_000;
    let limit = DEFAULT_CONTEXT_LIMIT;
    let health = context_health(tokens, limit);
    assert_eq!(health, ContextHealth::Red);
}

#[test]
fn test_context_health_zero_limit() {
    let health = context_health(100, 0);
    assert_eq!(health, ContextHealth::Green);
}

#[test]
fn test_context_usage_percent() {
    let tokens = 100_000;
    let limit = DEFAULT_CONTEXT_LIMIT;
    let percent = context_usage_percent(tokens, limit);
    assert_eq!(percent, 50.0);
}

#[test]
fn test_context_usage_percent_zero_limit() {
    let percent = context_usage_percent(100, 0);
    assert_eq!(percent, 0.0);
}

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
    let handlers = Handlers::new(config);
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
    let handlers = Handlers::new(config);
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.context_tokens = 50_000; // 25% - Green

    detection.detect_session_changes(&[session.clone()], &[], &handlers);

    session.context_tokens = 110_000; // 55% - Yellow (warning zone)
    let events = detection.detect_session_changes(&[session], &[], &handlers);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::SessionContextWarning {
        session_id,
        usage_percent,
    } = &events[0]
    {
        assert_eq!(session_id, "session-1");
        assert!(*usage_percent >= 50.0 && *usage_percent < 65.0);
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
    let handlers = Handlers::new(config);
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.context_tokens = 50_000; // 25% - Green

    detection.detect_session_changes(&[session.clone()], &[], &handlers);

    session.context_tokens = 130_000; // 65% - Red (critical threshold)
    let events = detection.detect_session_changes(&[session], &[], &handlers);
    assert_eq!(events.len(), 1);

    if let MonitorEvent::SessionContextCritical {
        session_id,
        usage_percent,
    } = &events[0]
    {
        assert_eq!(session_id, "session-1");
        assert!(*usage_percent >= 65.0);
    } else {
        panic!("Expected SessionContextCritical event");
    }
}

#[test]
fn test_parse_session_frontmatter() {
    let content = r#"---
id: session-abc-123
stage_id: stage-1
worktree_path: null
pid: 12345
status: running
context_tokens: 100000
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T01:00:00Z"
---

# Session Details
Test content
"#;

    let session = parse_session_from_markdown(content).expect("Should parse session");
    assert_eq!(session.id, "session-abc-123");
    assert_eq!(session.stage_id, Some("stage-1".to_string()));
    assert_eq!(session.status, SessionStatus::Running);
    assert_eq!(session.context_tokens, 100_000);
    assert_eq!(session.context_limit, 200_000);
}

#[test]
fn test_parse_stage_frontmatter() {
    let content = r#"---
id: stage-1
name: Test Stage
description: A test stage
status: executing
dependencies: []
parallel_group: null
acceptance: []
files: []
plan_id: null
worktree: null
session: session-1
parent_stage: null
child_stages: []
created_at: "2024-01-01T00:00:00Z"
updated_at: "2024-01-01T01:00:00Z"
completed_at: null
close_reason: null
---

# Stage Details
Test content
"#;

    let stage = parse_stage_from_markdown(content).expect("Should parse stage");
    assert_eq!(stage.id, "stage-1");
    assert_eq!(stage.name, "Test Stage");
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.session, Some("session-1".to_string()));
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

    let events = detection.detect_stage_changes(&[stage]);
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
}

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
    let handlers = Handlers::new(config);

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
    let handlers = Handlers::new(config);

    // Regular signal should not be detected as a merge session
    assert!(!handlers.is_merge_session("session-regular-123"));
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
fn test_check_session_alive_with_pid_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();

    let config = MonitorConfig {
        work_dir: work_dir.clone(),
        ..Default::default()
    };
    let handlers = Handlers::new(config);

    let mut session = Session::new();
    session.assign_to_stage("test-stage".to_string());
    session.set_pid(99999); // Non-existent PID

    // Without PID file, should use session.pid and return false (process doesn't exist)
    let result = handlers.check_session_alive(&session).unwrap();
    assert_eq!(result, Some(false));

    // Create PID file with current process PID (should be alive)
    let pids_dir = work_dir.join("pids");
    std::fs::create_dir_all(&pids_dir).unwrap();
    let pid_file = pids_dir.join("test-stage.pid");
    std::fs::write(&pid_file, std::process::id().to_string()).unwrap();

    // With PID file pointing to alive process, should return true
    let result = handlers.check_session_alive(&session).unwrap();
    assert_eq!(result, Some(true));

    // Write a non-existent PID to the file
    std::fs::write(&pid_file, "99998").unwrap();

    // With PID file pointing to dead process, should return false and clean up the file
    let result = handlers.check_session_alive(&session).unwrap();
    assert_eq!(result, Some(false));
    // PID file should be cleaned up after detecting dead process
    assert!(!pid_file.exists());
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
    let handlers = Handlers::new(config);
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
    let handlers = Handlers::new(config);
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
