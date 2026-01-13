//! Markdown parsing for session and stage files.
//!
//! Functions to parse Session and Stage structs from markdown content.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::parser::markdown::MarkdownDocument;

/// Parse a Session from markdown content
pub fn session_from_markdown(content: &str) -> Result<Session> {
    let doc =
        MarkdownDocument::parse(content).context("Failed to parse session markdown document")?;

    let id = doc
        .get_frontmatter("id")
        .ok_or_else(|| anyhow!("Missing 'id' in session frontmatter"))?
        .to_string();

    let stage_id = doc
        .get_frontmatter("stage_id")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != "null");

    let worktree_path = doc
        .get_frontmatter("worktree_path")
        .filter(|s| !s.is_empty() && *s != "null")
        .map(std::path::PathBuf::from);

    let pid = doc
        .get_frontmatter("pid")
        .and_then(|s| s.parse::<u32>().ok());

    let status_str = doc
        .get_frontmatter("status")
        .ok_or_else(|| anyhow!("Missing 'status' in session frontmatter"))?;

    let status = match status_str.as_str() {
        "spawning" => SessionStatus::Spawning,
        "running" => SessionStatus::Running,
        "paused" => SessionStatus::Paused,
        "completed" => SessionStatus::Completed,
        "crashed" => SessionStatus::Crashed,
        "context_exhausted" => SessionStatus::ContextExhausted,
        _ => bail!("Invalid session status: {status_str}"),
    };

    let context_tokens = doc
        .get_frontmatter("context_tokens")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let context_limit = doc
        .get_frontmatter("context_limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(200_000);

    let created_at = doc
        .get_frontmatter("created_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| anyhow!("Missing or invalid 'created_at' in session frontmatter"))?;

    let last_active = doc
        .get_frontmatter("last_active")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| anyhow!("Missing or invalid 'last_active' in session frontmatter"))?;

    // Parse merge session fields (with defaults for backward compatibility)
    let session_type = doc
        .get_frontmatter("session_type")
        .map(|s| match s.as_str() {
            "merge" => crate::models::session::SessionType::Merge,
            _ => crate::models::session::SessionType::Stage,
        })
        .unwrap_or_default();

    let merge_source_branch = doc.get_frontmatter("merge_source_branch").cloned();
    let merge_target_branch = doc.get_frontmatter("merge_target_branch").cloned();

    Ok(Session {
        id,
        stage_id,
        worktree_path,
        pid,
        status,
        context_tokens,
        context_limit,
        created_at,
        last_active,
        session_type,
        merge_source_branch,
        merge_target_branch,
    })
}

/// Parse a Stage from markdown content
pub fn stage_from_markdown(content: &str) -> Result<Stage> {
    let doc =
        MarkdownDocument::parse(content).context("Failed to parse stage markdown document")?;

    let id = doc
        .get_frontmatter("id")
        .ok_or_else(|| anyhow!("Missing 'id' in stage frontmatter"))?
        .to_string();

    let name = doc
        .get_frontmatter("name")
        .ok_or_else(|| anyhow!("Missing 'name' in stage frontmatter"))?
        .to_string();

    let description = doc
        .get_frontmatter("description")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let status_str = doc
        .get_frontmatter("status")
        .ok_or_else(|| anyhow!("Missing 'status' in stage frontmatter"))?;

    let status = match status_str.as_str() {
        "pending" => StageStatus::WaitingForDeps,
        "ready" => StageStatus::Queued,
        "executing" => StageStatus::Executing,
        "blocked" => StageStatus::Blocked,
        "completed" => StageStatus::Completed,
        "needs_handoff" => StageStatus::NeedsHandoff,
        "verified" => StageStatus::Completed, // Map legacy "verified" to Completed for backwards compatibility
        _ => bail!("Invalid stage status: {status_str}"),
    };

    let session = doc
        .get_frontmatter("session")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let created_at = doc
        .get_frontmatter("created_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(chrono::Utc::now);

    let updated_at = doc
        .get_frontmatter("updated_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(chrono::Utc::now);

    let completed_at = doc
        .get_frontmatter("completed_at")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    let close_reason = doc
        .get_frontmatter("close_reason")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let held = doc
        .get_frontmatter("held")
        .map(|s| s == "true")
        .unwrap_or(false);

    Ok(Stage {
        id,
        name,
        description,
        status,
        dependencies: Vec::new(),
        parallel_group: None,
        acceptance: Vec::new(),
        setup: Vec::new(),
        files: Vec::new(),
        stage_type: crate::models::stage::StageType::default(),
        plan_id: None,
        worktree: None,
        session,
        held,
        parent_stage: None,
        child_stages: Vec::new(),
        created_at,
        updated_at,
        completed_at,
        close_reason,
        auto_merge: None,
        working_dir: None,
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
        base_branch: None,
        base_merged_from: Vec::new(),
        outputs: Vec::new(),
        completed_commit: None,
        merged: false,
        merge_conflict: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_from_markdown() {
        let markdown = r#"---
id: session-123
stage_id: stage-456
status: running
context_tokens: 45000
context_limit: 200000
created_at: 2026-01-06T12:00:00Z
last_active: 2026-01-06T13:30:00Z
---

# Session: session-123
"#;

        let session = session_from_markdown(markdown).unwrap();
        assert_eq!(session.id, "session-123");
        assert_eq!(session.stage_id, Some("stage-456".to_string()));
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.context_tokens, 45000);
        assert_eq!(session.context_limit, 200000);
    }

    #[test]
    fn test_stage_from_markdown() {
        let markdown = r#"---
id: stage-123
name: Test Stage
description: A test stage
status: executing
session: session-456
created_at: 2026-01-06T12:00:00Z
updated_at: 2026-01-06T13:30:00Z
---

# Stage: Test Stage
"#;

        let stage = stage_from_markdown(markdown).unwrap();
        assert_eq!(stage.id, "stage-123");
        assert_eq!(stage.name, "Test Stage");
        assert_eq!(stage.description, Some("A test stage".to_string()));
        assert_eq!(stage.status, StageStatus::Executing);
        assert_eq!(stage.session, Some("session-456".to_string()));
    }
}
