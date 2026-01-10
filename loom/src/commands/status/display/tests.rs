use super::sessions::{is_pid_alive, is_session_orphaned};
use super::stages::parse_stage_from_markdown;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::StageStatus;

#[test]
fn test_is_session_orphaned_with_tmux_backend() {
    let mut session = Session::new();
    session.status = SessionStatus::Running;
    session.set_tmux_session("test-session".to_string());

    let _result = is_session_orphaned(&session);
}

#[test]
fn test_is_session_orphaned_with_native_backend() {
    let mut session = Session::new();
    session.status = SessionStatus::Running;
    session.set_pid(std::process::id());

    assert!(!is_session_orphaned(&session));

    session.set_pid(999999);
    assert!(is_session_orphaned(&session));
}

#[test]
fn test_is_session_orphaned_terminal_states() {
    let mut session = Session::new();
    session.set_pid(999999);

    session.status = SessionStatus::Completed;
    assert!(!is_session_orphaned(&session));

    session.status = SessionStatus::Crashed;
    assert!(!is_session_orphaned(&session));

    session.status = SessionStatus::ContextExhausted;
    assert!(!is_session_orphaned(&session));
}

#[test]
fn test_is_session_orphaned_no_backend_info() {
    let mut session = Session::new();
    session.status = SessionStatus::Running;

    assert!(!is_session_orphaned(&session));
}

#[test]
fn test_is_pid_alive_current_process() {
    let current_pid = std::process::id();
    assert!(is_pid_alive(current_pid));
}

#[test]
fn test_is_pid_alive_non_existent() {
    assert!(!is_pid_alive(999999));
}

#[test]
fn test_parse_stage_with_retry_info() {
    use crate::models::failure::FailureType;

    let content = r#"---
id: stage-test-1
name: Test Stage
status: blocked
dependencies: []
acceptance: []
setup: []
files: []
child_stages: []
retry_count: 2
max_retries: 3
created_at: 2025-01-10T12:00:00Z
updated_at: 2025-01-10T12:00:00Z
failure_info:
  failure_type: session-crash
  detected_at: 2025-01-10T12:00:00Z
  evidence:
    - "Session crashed unexpectedly"
---

# Stage: Test Stage
"#;

    let stage = parse_stage_from_markdown(content).expect("Should parse stage from markdown");

    assert_eq!(stage.id, "stage-test-1");
    assert_eq!(stage.name, "Test Stage");
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.retry_count, 2);
    assert_eq!(stage.max_retries, Some(3));
    assert!(stage.failure_info.is_some());

    if let Some(failure_info) = stage.failure_info {
        assert_eq!(failure_info.failure_type, FailureType::SessionCrash);
        assert_eq!(failure_info.evidence.len(), 1);
    }
}

#[test]
fn test_parse_stage_skipped() {
    let content = r#"---
id: stage-test-2
name: Skipped Stage
status: skipped
dependencies: []
acceptance: []
setup: []
files: []
child_stages: []
created_at: 2025-01-10T12:00:00Z
updated_at: 2025-01-10T12:00:00Z
---

# Stage: Skipped Stage
"#;

    let stage = parse_stage_from_markdown(content).expect("Should parse stage from markdown");

    assert_eq!(stage.id, "stage-test-2");
    assert_eq!(stage.name, "Skipped Stage");
    assert_eq!(stage.status, StageStatus::Skipped);
}
