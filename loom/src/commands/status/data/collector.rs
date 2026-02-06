use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;

use crate::commands::status::merge_status::build_merge_report;
use crate::fs::work_dir::WorkDir;
use crate::models::constants::STALENESS_THRESHOLD_SECS;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::get_merge_point;
use crate::orchestrator::monitor::heartbeat::{read_heartbeat, Heartbeat};
use crate::parser::frontmatter::parse_from_markdown;
use crate::process::is_process_alive;
use crate::verify::transitions::list_all_stages;

use super::{
    ActivityStatus, MergeSummary, ProgressSummary, SessionSummary, StageSummary, StatusData,
};

/// Read heartbeat for a specific stage from the heartbeat directory
fn read_heartbeat_for_stage(stage_id: &str, work_dir: &WorkDir) -> Option<Heartbeat> {
    let heartbeat_path = work_dir
        .root()
        .join("heartbeat")
        .join(format!("{stage_id}.json"));
    if heartbeat_path.exists() {
        read_heartbeat(&heartbeat_path).ok()
    } else {
        None
    }
}

/// Calculate activity status from session state and heartbeat staleness
fn determine_activity_status(
    session: Option<&Session>,
    staleness_secs: Option<u64>,
) -> ActivityStatus {
    match (session, staleness_secs) {
        // No session - idle
        (None, _) => ActivityStatus::Idle,
        // Session crashed
        (Some(s), _) if s.status == SessionStatus::Crashed => ActivityStatus::Error,
        // Session running but stale heartbeat (> 5 minutes)
        (Some(_), Some(secs)) if secs > STALENESS_THRESHOLD_SECS => ActivityStatus::Stale,
        // Session running with recent heartbeat
        (Some(_), _) => ActivityStatus::Working,
    }
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

    parse_from_markdown(&content, "Session")
}

/// Build a StageSummary from a Stage and optional associated Session
fn build_stage_summary(stage: &Stage, sessions: &[Session], work_dir: &WorkDir) -> StageSummary {
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

    // Read heartbeat for this stage
    let heartbeat = read_heartbeat_for_stage(&stage.id, work_dir);

    // Calculate staleness (seconds since last heartbeat)
    let staleness_secs = heartbeat.as_ref().map(|hb| {
        let age = Utc::now().signed_duration_since(hb.timestamp);
        age.num_seconds().max(0) as u64
    });

    // Determine activity status based on session and heartbeat
    let activity_status = determine_activity_status(session, staleness_secs);

    // Extract heartbeat details
    let last_tool = heartbeat.as_ref().and_then(|hb| hb.last_tool.clone());
    let last_activity = heartbeat.as_ref().and_then(|hb| hb.activity.clone());

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
        activity_status,
        last_tool,
        last_activity,
        staleness_secs,
        context_budget_pct: None, // TODO: Read from plan if needed
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
        .map(|stage| build_stage_summary(stage, &sessions, work_dir))
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
            verification_status: Default::default(),
            context_budget: None,
            truths: Vec::new(),
            artifacts: Vec::new(),
            wiring: Vec::new(),
            sandbox: Default::default(),
            execution_mode: None,
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
        session.context_limit = 200000;

        let summary = build_stage_summary(&stage, &[session], &work_dir);

        assert_eq!(summary.id, "test-stage");
        assert_eq!(summary.status, StageStatus::Executing);
        assert_eq!(summary.dependencies, vec!["dep-1"]);
        assert!(summary.context_pct.is_some());
        assert_eq!(summary.context_pct.unwrap(), 0.25); // 50000/200000
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
        assert!(summary.context_pct.is_none());
        assert!(summary.elapsed_secs.is_some());
        // New fields
        assert_eq!(summary.activity_status, ActivityStatus::Idle);
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
}
