//! Tests for state transition commands

use super::super::state::{block, hold, release, reset, resume_from_waiting, waiting};
use super::{create_test_stage, save_test_stage, setup_work_dir};
use crate::fs::session_files::save_session;
use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::transitions::load_stage;
use chrono::Utc;
use serial_test::serial;

/// Pins `LOOM_TERMINAL` for a test's duration and restores it on drop.
///
/// `reset()` now always asks the session registry for live agents, which
/// constructs a `SessionBackend` and, on the default `native` config, that
/// eagerly runs `detect_terminal()`. Headless CI runners without any
/// terminal emulator installed would otherwise fail every `reset()` test for
/// a reason unrelated to what each test actually checks. Same technique as
/// `tests/e2e/daemon_config/stale_project_execution.rs`.
struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
#[serial]
fn test_block_updates_status() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let stage = create_test_stage("test-stage", StageStatus::Queued);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = block("test-stage".to_string(), "Test blocker".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "block() failed: {:?}", result.err());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Blocked);
    assert_eq!(loaded_stage.close_reason, Some("Test blocker".to_string()));
}

#[test]
#[serial]
fn test_reset_clears_completion() {
    let _terminal_env = EnvVarGuard::set("LOOM_TERMINAL", "xterm");
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let mut stage = create_test_stage("test-stage", StageStatus::Completed);
    stage.completed_at = Some(Utc::now());
    stage.close_reason = Some("Done".to_string());
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = reset("test-stage".to_string(), false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "reset() failed: {:?}", result.err());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::WaitingForDeps);
    assert_eq!(loaded_stage.completed_at, None);
    assert_eq!(loaded_stage.close_reason, None);
}

#[test]
#[serial]
fn test_reset_hard_clears_session() {
    let _terminal_env = EnvVarGuard::set("LOOM_TERMINAL", "xterm");
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.session = Some("session-1".to_string());
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = reset("test-stage".to_string(), true, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.session, None);
}

#[test]
#[serial]
fn test_reset_soft_also_clears_session() {
    let _terminal_env = EnvVarGuard::set("LOOM_TERMINAL", "xterm");
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.session = Some("session-1".to_string());
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = reset("test-stage".to_string(), false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "reset() failed: {:?}", result.err());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::WaitingForDeps);
    assert_eq!(
        loaded_stage.session, None,
        "a soft reset must clear stage.session too, or the reset stage still names a session"
    );
}

#[test]
#[serial]
fn test_reset_proceeds_when_nothing_is_live() {
    let _terminal_env = EnvVarGuard::set("LOOM_TERMINAL", "xterm");
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // stage.session names an id, but no session file and no PID evidence
    // exist for it - nothing is actually live.
    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.session = Some("stale-session".to_string());
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = reset("test-stage".to_string(), false, false);

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok(), "reset() failed: {:?}", result.err());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::WaitingForDeps);
    assert_eq!(loaded_stage.session, None);
}

#[test]
#[serial]
fn test_reset_refuses_with_live_session_and_no_kill_flag() {
    let _terminal_env = EnvVarGuard::set("LOOM_TERMINAL", "xterm");
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.session = Some("live-session-1".to_string());
    save_test_stage(&work_dir_path, &stage);

    // A genuinely live session: a real session file assigned to the stage,
    // plus PID identity evidence pointing at this test process itself so
    // the liveness check verifies alive.
    let mut session = Session::new();
    session.id = "live-session-1".to_string();
    session.assign_to_stage("test-stage".to_string());
    save_session(&session, &work_dir_path).unwrap();
    write_test_pid_identity(&work_dir_path, &session, std::process::id()).unwrap();

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = reset("test-stage".to_string(), false, false);

    std::env::set_current_dir(original_dir).unwrap();

    let err = result.expect_err("reset() must refuse while an agent is still live");
    assert!(
        err.to_string().contains("live-session-1"),
        "refusal must name the live session id, got: {err}"
    );

    // Nothing about the stage changed - the reset never applied.
    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Executing);
    assert_eq!(loaded_stage.session, Some("live-session-1".to_string()));
}

#[test]
#[serial]
fn test_hold_sets_held_flag() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let stage = create_test_stage("test-stage", StageStatus::Queued);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = hold("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert!(loaded_stage.held);
}

#[test]
#[serial]
fn test_release_clears_held_flag() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let mut stage = create_test_stage("test-stage", StageStatus::Queued);
    stage.held = true;
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = release("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert!(!loaded_stage.held);
}

#[test]
#[serial]
fn test_waiting_transitions_from_executing() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let stage = create_test_stage("test-stage", StageStatus::Executing);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = waiting("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::WaitingForInput);
}

#[test]
#[serial]
fn test_waiting_skips_if_not_executing() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let stage = create_test_stage("test-stage", StageStatus::Queued);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = waiting("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Queued);
}

#[test]
#[serial]
fn test_resume_from_waiting_transitions_to_executing() {
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    let stage = create_test_stage("test-stage", StageStatus::WaitingForInput);
    save_test_stage(&work_dir_path, &stage);

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = resume_from_waiting("test-stage".to_string());

    std::env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());

    let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Executing);
}
