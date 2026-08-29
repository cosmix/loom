//! Frontmatter parsing for the files the monitor polls.

use crate::models::session::SessionStatus;
use crate::models::stage::StageStatus;
use crate::orchestrator::monitor::core::parse_session_from_markdown;
use crate::verify::transitions::parse_stage_from_markdown;

#[test]
fn test_parse_session_frontmatter() {
    let content = r#"---
id: session-abc-123
stage_id: stage-1
worktree_path: null
pid: 12345
status: running
context_tokens: 100000
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
