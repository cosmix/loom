use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;

use crate::commands::status::merge_status::build_merge_report;
use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::get_merge_point;
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
    let frontmatter = extract_yaml_frontmatter(content)?;

    let session: Session = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Session from frontmatter")?;

    Ok(session)
}

/// Extract YAML frontmatter from markdown content
fn extract_yaml_frontmatter(content: &str) -> Result<serde_yaml::Value> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || lines[0] != "---" {
        anyhow::bail!("Missing YAML frontmatter delimiter");
    }

    let end_index = lines[1..]
        .iter()
        .position(|&line| line == "---")
        .ok_or_else(|| anyhow::anyhow!("Missing closing YAML frontmatter delimiter"))?
        + 1;

    let yaml_content = lines[1..end_index].join("\n");
    serde_yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")
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
        context_pct,
        elapsed_secs: Some(elapsed_secs),
        base_branch: stage.base_branch.clone(),
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
