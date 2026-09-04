//! Tests for daemon server module.

use super::super::protocol::Response;
use super::core::{daemon_running_from_status, DaemonServer, DaemonStatus};
use super::status::collect_status;
use crate::models::stage::StageStatus;
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

#[test]
fn test_new_daemon_server() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    let server = DaemonServer::new(work_dir);

    assert_eq!(server.socket_path, work_dir.join("orchestrator.sock"));
    assert_eq!(server.log_path, work_dir.join("orchestrator.log"));
}

#[test]
fn test_is_running_no_pid_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    assert!(!DaemonServer::is_running(work_dir));
}

#[test]
fn free_lock_overrides_live_legacy_pid_and_cleans_stale_state() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();
    fs::write(
        work_dir.join("orchestrator.lock"),
        std::process::id().to_string(),
    )
    .unwrap();
    fs::write(
        work_dir.join("orchestrator.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    fs::write(work_dir.join("orchestrator.sock"), b"stale").unwrap();
    fs::write(work_dir.join("admin.token"), b"stale").unwrap();

    assert_eq!(
        DaemonServer::check_status(work_dir),
        DaemonStatus::NotRunning
    );
    assert!(!work_dir.join("orchestrator.pid").exists());
    assert!(!work_dir.join("orchestrator.sock").exists());
    assert!(!work_dir.join("admin.token").exists());
}

#[test]
fn test_read_pid_valid() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();
    let pid_path = work_dir.join("orchestrator.pid");

    fs::write(&pid_path, "12345").expect("Failed to write PID file");

    let pid = DaemonServer::read_pid(work_dir);
    assert_eq!(pid, Some(12345));
}

#[test]
fn test_read_pid_invalid() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();
    let pid_path = work_dir.join("orchestrator.pid");

    fs::write(&pid_path, "not-a-number").expect("Failed to write PID file");

    let pid = DaemonServer::read_pid(work_dir);
    assert_eq!(pid, None);
}

#[test]
fn test_read_pid_no_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    let pid = DaemonServer::read_pid(work_dir);
    assert_eq!(pid, None);
}

#[test]
fn unreachable_status_counts_as_running() {
    // `Unreachable` is only ever produced when the singleton flock is
    // already `Held` (see `DaemonServer::check_status`), so a live daemon
    // genuinely owns this `.loom/work/` — the failed `connect()` is a property of
    // the sandboxed caller, not evidence the daemon died. `is_running` must
    // treat it as running so a second `loom run` cannot start against the
    // same `.loom/work/` (the singleton hazard recorded in
    // doc/loom/knowledge/concerns/daemon-singleton.md). Simulating a real
    // `PermissionDenied` from `connect()` isn't practical in a unit test, so
    // this asserts the pure classification directly instead of driving it
    // through a real socket.
    assert!(daemon_running_from_status(DaemonStatus::Unreachable));
}

#[test]
fn process_only_and_not_running_still_classify_correctly() {
    // Guards against a future edit accidentally widening/narrowing the
    // `matches!` in `daemon_running_from_status` while adding new variants.
    assert!(daemon_running_from_status(DaemonStatus::Running));
    assert!(daemon_running_from_status(DaemonStatus::ProcessOnly));
    assert!(!daemon_running_from_status(DaemonStatus::NotRunning));
}

#[test]
fn test_shutdown_flag() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    let server = DaemonServer::new(work_dir);
    assert!(!server.shutdown_flag.load(Ordering::Relaxed));

    server.shutdown();
    assert!(server.shutdown_flag.load(Ordering::Relaxed));
}

#[test]
fn test_collect_status_empty_dir() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let wd = crate::fs::work_dir::WorkDir::new(temp_dir.path().join(".loom/work")).unwrap();
    fs::create_dir_all(wd.root().join("stages")).unwrap();

    let result = collect_status(wd.root());
    assert!(result.is_ok());

    match result.unwrap() {
        Response::StatusUpdate { data } => assert!(data.stages.is_empty()),
        _ => panic!("Expected StatusUpdate response"),
    }
}

#[test]
fn test_collect_status_with_stages() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let wd = crate::fs::work_dir::WorkDir::new(temp_dir.path().join(".loom/work")).unwrap();
    let stages_dir = wd.root().join("stages");
    fs::create_dir_all(&stages_dir).expect("Failed to create stages dir");

    // Create a pending stage
    let pending_stage = r#"---
id: stage-pending
name: Pending Stage
status: pending
session: ~
dependencies: []
acceptance: []
files: []
child_stages: []
created_at: "2024-01-01T00:00:00Z"
updated_at: "2024-01-01T00:00:00Z"
---
"#;
    fs::write(stages_dir.join("stage-pending.md"), pending_stage).expect("Failed to write stage");

    // Create an executing stage
    let executing_stage = r#"---
id: stage-executing
name: Executing Stage
status: executing
session: session-1
dependencies: []
acceptance: []
files: []
child_stages: []
created_at: "2024-01-01T00:00:00Z"
updated_at: "2024-01-01T00:00:00Z"
---
"#;
    fs::write(stages_dir.join("stage-executing.md"), executing_stage)
        .expect("Failed to write stage");

    // Create a completed stage
    let completed_stage = r#"---
id: stage-completed
name: Completed Stage
status: completed
session: ~
dependencies: []
acceptance: []
files: []
child_stages: []
created_at: "2024-01-01T00:00:00Z"
updated_at: "2024-01-01T00:00:00Z"
---
"#;
    fs::write(stages_dir.join("stage-completed.md"), completed_stage)
        .expect("Failed to write stage");

    let result = collect_status(wd.root());
    assert!(result.is_ok());

    match result.unwrap() {
        Response::StatusUpdate { data } => {
            assert_eq!(data.stages.len(), 3);
            assert!(data.stages.iter().any(
                |stage| stage.id == "stage-executing" && stage.status == StageStatus::Executing
            ));
            assert!(data
                .stages
                .iter()
                .any(|stage| stage.id == "stage-pending"
                    && stage.status == StageStatus::WaitingForDeps));
            assert!(data.stages.iter().any(
                |stage| stage.id == "stage-completed" && stage.status == StageStatus::Completed
            ));
        }
        _ => panic!("Expected StatusUpdate response"),
    }
}
