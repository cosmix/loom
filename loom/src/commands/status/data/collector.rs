use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;

use crate::commands::status::merge_status::build_merge_report;
use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::get_merge_point;
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::list_all_stages;

use super::{MergeSummary, ProgressSummary, SessionSummary, StageSummary, StatusData};

/// Check if a process is alive by checking /proc/{pid}
fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Load all sessions from .work/sessions/ directory
fn load_all_sessions(work_dir: &WorkDir) -> Result<Vec<Session>> {
    let sessions_dir = work_dir.sessions_dir();
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_session_from_file(&path) {
                Ok(session) => sessions.push(session),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load session from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(sessions)
}

/// Load a single session from a markdown file
fn load_session_from_file(path: &std::path::Path) -> Result<Session> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read session file: {}", path.display()))?;

    parse_session_from_markdown(&content)
}

/// Parse a Session from markdown with YAML frontmatter
fn parse_session_from_markdown(content: &str) -> Result<Session> {
    parse_from_markdown(content, "Session")
}

/// Build a StageSummary from a Stage and optional associated Session
fn build_stage_summary(stage: &Stage, sessions: &[Session]) -> StageSummary {
    let session = sessions
        .iter()
        .find(|s| s.stage_id.as_ref() == Some(&stage.id));

    let context_pct = session.map(|s| {
        if s.context_limit == 0 {
            0.0
        } else {
            s.context_tokens as f32 / s.context_limit as f32
        }
    });

    let elapsed_secs = (Utc::now() - stage.created_at).num_seconds();

    StageSummary {
        id: stage.id.clone(),
        name: stage.name.clone(),
        status: stage.status.clone(),
        dependencies: stage.dependencies.clone(),
        context_pct,
        elapsed_secs: Some(elapsed_secs),
        base_branch: stage.base_branch.clone(),
        base_merged_from: stage.base_merged_from.clone(),
        failure_info: stage.failure_info.clone(),
    }
}

/// Build a SessionSummary from a Session
fn build_session_summary(session: &Session) -> SessionSummary {
    let uptime_secs = (Utc::now() - session.created_at).num_seconds();
    let is_alive = session.pid.map(is_process_alive).unwrap_or(false);

    SessionSummary {
        id: session.id.clone(),
        stage_id: session.stage_id.clone(),
        pid: session.pid,
        context_tokens: session.context_tokens,
        context_limit: session.context_limit,
        uptime_secs,
        is_alive,
    }
}

/// Calculate progress summary from stages
fn calculate_progress(stages: &[Stage]) -> ProgressSummary {
    let total = stages.len();
    let mut completed = 0;
    let mut executing = 0;
    let mut pending = 0;
    let mut blocked = 0;

    for stage in stages {
        match stage.status {
            StageStatus::Completed => completed += 1,
            StageStatus::Executing => executing += 1,
            StageStatus::WaitingForDeps | StageStatus::Queued => pending += 1,
            StageStatus::Blocked
            | StageStatus::MergeConflict
            | StageStatus::CompletedWithFailures
            | StageStatus::MergeBlocked => blocked += 1,
            StageStatus::NeedsHandoff | StageStatus::WaitingForInput => executing += 1,
            StageStatus::Skipped => {}
        }
    }

    ProgressSummary {
        total,
        completed,
        executing,
        pending,
        blocked,
    }
}

/// Build a MergeSummary from merge report
fn build_merge_summary_from_report(
    report: &crate::commands::status::merge_status::MergeStatusReport,
) -> MergeSummary {
    MergeSummary {
        merged: report.merged.clone(),
        pending: report.pending.clone(),
        conflicts: report.conflicts.clone(),
    }
}

/// Collect all status data from the work directory
pub fn collect_status_data(work_dir: &WorkDir) -> Result<StatusData> {
    // Load all stages
    let stages = list_all_stages(work_dir.root())?;

    // Load all sessions
    let sessions = load_all_sessions(work_dir)?;

    // Build stage summaries
    let stage_summaries: Vec<StageSummary> = stages
        .iter()
        .map(|stage| build_stage_summary(stage, &sessions))
        .collect();

    // Build session summaries
    let session_summaries: Vec<SessionSummary> =
        sessions.iter().map(build_session_summary).collect();

    // Get merge point for merge report
    let merge_point = if let Some(project_root) = work_dir.project_root() {
        get_merge_point(project_root).unwrap_or_else(|_| "main".to_string())
    } else {
        "main".to_string()
    };

    // Build merge report
    let merge_report = if let Some(project_root) = work_dir.project_root() {
        build_merge_report(&stages, &merge_point, project_root)?
    } else {
        crate::commands::status::merge_status::MergeStatusReport::new()
    };

    let merge_summary = build_merge_summary_from_report(&merge_report);

    // Calculate progress
    let progress = calculate_progress(&stages);

    Ok(StatusData {
        stages: stage_summaries,
        sessions: session_summaries,
        merge: merge_summary,
        progress,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageType;
    use chrono::Utc;

    fn make_test_stage(id: &str, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
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
            merged: false,
            merge_conflict: false,
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
        let mut stage = make_test_stage("test-stage", StageStatus::Executing);
        stage.dependencies = vec!["dep-1".to_string()];
        let mut session = Session::new();
        session.assign_to_stage("test-stage".to_string());
        session.context_tokens = 50000;
        session.context_limit = 200000;

        let summary = build_stage_summary(&stage, &[session]);

        assert_eq!(summary.id, "test-stage");
        assert_eq!(summary.status, StageStatus::Executing);
        assert_eq!(summary.dependencies, vec!["dep-1"]);
        assert!(summary.context_pct.is_some());
        assert_eq!(summary.context_pct.unwrap(), 0.25); // 50000/200000
        assert!(summary.elapsed_secs.is_some());
    }

    #[test]
    fn test_build_stage_summary_without_session() {
        let stage = make_test_stage("test-stage", StageStatus::WaitingForDeps);

        let summary = build_stage_summary(&stage, &[]);

        assert_eq!(summary.id, "test-stage");
        assert_eq!(summary.status, StageStatus::WaitingForDeps);
        assert!(summary.dependencies.is_empty());
        assert!(summary.context_pct.is_none());
        assert!(summary.elapsed_secs.is_some());
    }

    #[test]
    fn test_build_session_summary() {
        let mut session = Session::new();
        session.assign_to_stage("test-stage".to_string());
        session.pid = Some(12345);
        session.context_tokens = 100000;
        session.context_limit = 200000;

        let summary = build_session_summary(&session);

        assert_eq!(summary.stage_id, Some("test-stage".to_string()));
        assert_eq!(summary.pid, Some(12345));
        assert_eq!(summary.context_tokens, 100000);
        assert_eq!(summary.context_limit, 200000);
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
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session content"#;

        let result = parse_session_from_markdown(content);
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.id, "test-session");
    }

    #[test]
    fn test_parse_session_from_markdown_missing_delimiter() {
        let content = r#"id: test
status: executing"#;

        let result = parse_session_from_markdown(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No frontmatter delimiter"));
    }
}
