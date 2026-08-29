use super::*;
use crate::models::stage::{Implementers, StageType};
use chrono::Utc;

fn make_test_stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        code_review: None,
        status,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        stage_type: StageType::default(),
        plan_id: None,
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
        started_at: None,
        duration_secs: None,
        execution_secs: None,
        attempt_started_at: None,
        close_reason: None,
        auto_merge: None,
        working_dir: Some(".".to_string()),
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
        base_branch: None,
        base_merged_from: vec![],
        outputs: vec![],
        completed_commit: None,
        cleanup_warning: None,
        merged: false,
        merge_conflict: false,
        verification_status: Default::default(),
        context_ceiling_tokens: None,
        plan_overview: None,
        artifacts: Vec::new(),
        wiring: Vec::new(),
        wiring_tests: Vec::new(),
        dead_code_check: None,
        before_stage: Vec::new(),
        after_stage: Vec::new(),
        fix_attempts: 0,
        dispute_count: 0,
        evidence_rounds: 0,
        amendments_applied: 0,
        sandbox: Default::default(),
        execution_mode: None,
        max_fix_attempts: None,
        review_reason: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    }
}

#[test]
fn test_calculate_progress() {
    let stages = vec![
        make_test_stage("stage-1", StageStatus::Completed),
        make_test_stage("stage-2", StageStatus::Executing),
        make_test_stage("stage-3", StageStatus::WaitingForDeps),
        make_test_stage("stage-4", StageStatus::Queued),
        make_test_stage("stage-5", StageStatus::Blocked),
    ];

    let progress = calculate_progress(&stages);

    assert_eq!(progress.total, 5);
    assert_eq!(progress.completed, 1);
    assert_eq!(progress.executing, 1);
    assert_eq!(progress.pending, 2); // WaitingForDeps + Queued
    assert_eq!(progress.blocked, 1);
}

#[test]
fn test_calculate_progress_with_needs_handoff() {
    let stages = vec![
        make_test_stage("stage-1", StageStatus::NeedsHandoff),
        make_test_stage("stage-2", StageStatus::WaitingForInput),
    ];

    let progress = calculate_progress(&stages);

    assert_eq!(progress.total, 2);
    assert_eq!(progress.executing, 2); // Both count as executing
}

#[test]
fn test_calculate_progress_with_failures() {
    let stages = vec![
        make_test_stage("stage-1", StageStatus::CompletedWithFailures),
        make_test_stage("stage-2", StageStatus::MergeConflict),
        make_test_stage("stage-3", StageStatus::MergeBlocked),
    ];

    let progress = calculate_progress(&stages);

    assert_eq!(progress.total, 3);
    assert_eq!(progress.blocked, 3); // All count as blocked
}

#[test]
fn test_build_stage_summary_with_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let work_dir = WorkDir::new(tmp.path()).unwrap();
    work_dir.initialize().unwrap();

    let mut stage = make_test_stage("test-stage", StageStatus::Executing);
    stage.dependencies = vec!["dep-1".to_string()];
    let mut session = Session::new();
    session.assign_to_stage("test-stage".to_string());
    session.context_tokens = 50000;

    let summary = build_stage_summary(&stage, &[session], &work_dir);

    assert_eq!(summary.id, "test-stage");
    assert_eq!(summary.status, StageStatus::Executing);
    assert_eq!(summary.dependencies, vec!["dep-1"]);
    assert_eq!(summary.context_tokens, Some(50_000));
    assert_eq!(summary.context_ceiling_tokens, Some(150_000));
    assert!(summary.elapsed_secs.is_some());
    // New fields
    assert_eq!(summary.activity_status, ActivityStatus::Working);
    assert!(summary.staleness_secs.is_none()); // No heartbeat file
}

#[test]
fn test_build_stage_summary_without_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let work_dir = WorkDir::new(tmp.path()).unwrap();
    work_dir.initialize().unwrap();

    let stage = make_test_stage("test-stage", StageStatus::WaitingForDeps);

    let summary = build_stage_summary(&stage, &[], &work_dir);

    assert_eq!(summary.id, "test-stage");
    assert_eq!(summary.status, StageStatus::WaitingForDeps);
    assert!(summary.dependencies.is_empty());
    assert_eq!(summary.context_tokens, None);
    assert!(summary.elapsed_secs.is_some());
    // New fields
    assert_eq!(summary.activity_status, ActivityStatus::Idle);
}

#[test]
fn test_build_stage_summary_orphaned_when_executing_without_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let work_dir = WorkDir::new(tmp.path()).unwrap();
    work_dir.initialize().unwrap();

    // Stage claims Executing but no session names it - a killed daemon
    // or lost session file, not a quiet stage.
    let stage = make_test_stage("orphan-stage", StageStatus::Executing);

    let summary = build_stage_summary(&stage, &[], &work_dir);

    assert_eq!(summary.activity_status, ActivityStatus::Orphaned);
}

#[test]
fn test_build_session_summary() {
    let mut session = Session::new();
    session.assign_to_stage("test-stage".to_string());
    session.pid = Some(12345);
    session.context_tokens = 100000;

    let summary = build_session_summary(&session);

    assert_eq!(summary.stage_id, Some("test-stage".to_string()));
    assert_eq!(summary.pid, Some(12345));
    assert_eq!(summary.context_tokens, 100000);
    assert!(summary.uptime_secs >= 0);
}

#[test]
fn test_build_merge_summary_from_report() {
    let mut report = crate::commands::status::merge_status::MergeStatusReport::new();
    report.merged.push("stage-1".to_string());
    report.pending.push("stage-2".to_string());
    report.conflicts.push("stage-3".to_string());

    let summary = build_merge_summary_from_report(&report);

    assert_eq!(summary.merged, vec!["stage-1"]);
    assert_eq!(summary.pending, vec!["stage-2"]);
    assert_eq!(summary.conflicts, vec!["stage-3"]);
}

#[test]
fn test_parse_session_from_markdown() {
    let content = r#"---
id: test-session
status: running
context_tokens: 1000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session content"#;

    let result: Result<Session> = parse_from_markdown(content, "Session");
    assert!(result.is_ok());
    let session = result.unwrap();
    assert_eq!(session.id, "test-session");
}

#[test]
fn test_parse_session_from_markdown_missing_delimiter() {
    let content = r#"id: test
status: executing"#;

    let result: Result<Session> = parse_from_markdown(content, "Session");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No frontmatter delimiter"));
}
