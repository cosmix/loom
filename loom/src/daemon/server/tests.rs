//! Tests for daemon server module.

use super::super::protocol::Response;
use super::core::DaemonServer;
use super::status::{
    collect_status, detect_worktree_status, is_manually_merged, parse_stage_frontmatter,
};
use crate::models::worktree::WorktreeStatus;
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

#[test]
fn test_new_daemon_server() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    let server = DaemonServer::new(work_dir);

    assert_eq!(server.socket_path, work_dir.join("orchestrator.sock"));
    assert_eq!(server.pid_path, work_dir.join("orchestrator.pid"));
    assert_eq!(server.log_path, work_dir.join("orchestrator.log"));
}

#[test]
fn test_is_running_no_pid_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    assert!(!DaemonServer::is_running(work_dir));
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
fn test_shutdown_flag() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    let server = DaemonServer::new(work_dir);
    assert!(!server.shutdown_flag.load(Ordering::Relaxed));

    server.shutdown();
    assert!(server.shutdown_flag.load(Ordering::Relaxed));
}

#[test]
fn test_parse_stage_frontmatter_valid() {
    let content = r#"---
id: stage-1
name: Test Stage
status: executing
session: session-123
---

# Stage content
"#;

    let result = parse_stage_frontmatter(content);
    assert!(result.is_some());

    let (id, name, status, session) = result.unwrap();
    assert_eq!(id, "stage-1");
    assert_eq!(name, "Test Stage");
    assert_eq!(status, "executing");
    assert_eq!(session, Some("session-123".to_string()));
}

#[test]
fn test_parse_stage_frontmatter_no_session() {
    let content = r#"---
id: stage-2
name: Another Stage
status: pending
session: ~
---

# Stage content
"#;

    let result = parse_stage_frontmatter(content);
    assert!(result.is_some());

    let (id, name, status, session) = result.unwrap();
    assert_eq!(id, "stage-2");
    assert_eq!(name, "Another Stage");
    assert_eq!(status, "pending");
    assert!(session.is_none());
}

#[test]
fn test_parse_stage_frontmatter_missing_frontmatter() {
    let content = "# No frontmatter here";
    let result = parse_stage_frontmatter(content);
    assert!(result.is_none());
}

#[test]
fn test_collect_status_empty_dir() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();

    let result = collect_status(work_dir);
    assert!(result.is_ok());

    match result.unwrap() {
        Response::StatusUpdate {
            stages_executing,
            stages_pending,
            stages_completed,
            stages_blocked,
        } => {
            assert!(stages_executing.is_empty());
            assert!(stages_pending.is_empty());
            assert!(stages_completed.is_empty());
            assert!(stages_blocked.is_empty());
        }
        _ => panic!("Expected StatusUpdate response"),
    }
}

#[test]
fn test_collect_status_with_stages() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path();
    let stages_dir = work_dir.join("stages");
    fs::create_dir_all(&stages_dir).expect("Failed to create stages dir");

    // Create a pending stage
    let pending_stage = r#"---
id: stage-pending
name: Pending Stage
status: pending
session: ~
---
"#;
    fs::write(stages_dir.join("stage-pending.md"), pending_stage).expect("Failed to write stage");

    // Create an executing stage
    let executing_stage = r#"---
id: stage-executing
name: Executing Stage
status: executing
session: session-1
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
---
"#;
    fs::write(stages_dir.join("stage-completed.md"), completed_stage)
        .expect("Failed to write stage");

    let result = collect_status(work_dir);
    assert!(result.is_ok());

    match result.unwrap() {
        Response::StatusUpdate {
            stages_executing,
            stages_pending,
            stages_completed,
            stages_blocked,
        } => {
            assert_eq!(stages_executing.len(), 1);
            assert_eq!(stages_executing[0].id, "stage-executing");
            assert_eq!(stages_pending.len(), 1);
            assert!(stages_pending.contains(&"stage-pending".to_string()));
            assert_eq!(stages_completed.len(), 1);
            assert!(stages_completed.contains(&"stage-completed".to_string()));
            assert!(stages_blocked.is_empty());
        }
        _ => panic!("Expected StatusUpdate response"),
    }
}

#[test]
fn test_detect_worktree_status_no_worktree() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_root = temp_dir.path();

    // When worktree doesn't exist, should return None
    let status = detect_worktree_status("nonexistent-stage", repo_root);
    assert!(status.is_none());
}

#[test]
fn test_detect_worktree_status_active() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_root = temp_dir.path();

    // Create a worktree directory (without git operations)
    let worktree_path = repo_root.join(".worktrees").join("test-stage");
    fs::create_dir_all(&worktree_path).expect("Failed to create worktree dir");

    // Create a .git file pointing to a gitdir (simulating a worktree)
    let git_file = worktree_path.join(".git");
    fs::write(&git_file, "gitdir: /nonexistent/path").expect("Failed to write .git file");

    // Since this is not a real git repo, is_manually_merged will return false
    // and there's no MERGE_HEAD, so status should be Active
    let status = detect_worktree_status("test-stage", repo_root);
    assert_eq!(status, Some(WorktreeStatus::Active));
}

#[test]
fn test_is_manually_merged_no_git_repo() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_root = temp_dir.path();

    // When not in a git repo, is_manually_merged should gracefully return false
    let result = is_manually_merged("test-stage", repo_root);
    assert!(!result);
}

// Note: Testing is_manually_merged with a real git repo and merged branches
// requires complex setup and is better suited for e2e tests.
// The function:
// 1. Gets the default branch (main/master)
// 2. Checks if loom/{stage_id} is in `git branch --merged {target}`
// 3. Returns true if the branch has been merged, false otherwise
