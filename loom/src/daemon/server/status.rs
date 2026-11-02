//! Status collection and completion summaries.

use super::super::protocol::{CompletionSummary, Response, StageCompletionInfo};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

use crate::models::stage::{Stage, StatusBucket};
use crate::parser::frontmatter::parse_from_markdown;

/// Persisted [`TerminalOutcome`] of the latest run.
pub(super) const COMPLETION_MARKER: &str = "orchestrator.complete";
/// Plain-text scheduler failure, for operators reading the work directory.
pub(super) const SCHEDULER_FAILURE_FILE: &str = "scheduler.failed";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub(super) enum TerminalOutcome {
    Success,
    SchedulerFailure(String),
    FinalizationFailure(String),
    Incomplete(String),
}

/// Collect current stage status from the work directory.
pub fn collect_status(work_dir: &Path) -> Result<Response> {
    let wd = crate::fs::work_dir::WorkDir::new(work_dir)
        .with_context(|| format!("Failed to resolve work dir: {}", work_dir.display()))?;
    let data = crate::status::data::collect_status_data(&wd)?;
    Ok(Response::StatusUpdate {
        data: Box::new(data),
    })
}

/// Collect completion summary from all stage files.
///
/// Gathers timing information and final status for all stages,
/// calculates total duration and success/failure counts.
///
/// # Arguments
/// * `work_dir` - The .loom/work/ directory path
///
/// # Returns
/// A CompletionSummary with all stage completion information
pub fn collect_completion_summary(work_dir: &Path) -> Result<CompletionSummary> {
    collect_completion_with_details(work_dir).map(|(summary, _)| summary)
}

pub(crate) fn completion_diagnostics(work_dir: &Path) -> Option<String> {
    if !work_dir.join(COMPLETION_MARKER).exists() {
        return None;
    }
    match collect_completion_with_details(work_dir) {
        Ok((_, details)) if details.is_empty() => None,
        Ok((_, details)) => Some(details.join("; ")),
        Err(error) => Some(format!("Cannot collect completion summary: {error:#}")),
    }
}

/// The frames announcing a finished run: any failure detail, then the summary.
pub(super) fn completion_frames(work_dir: &Path) -> Result<Vec<Response>> {
    let (summary, details) = collect_completion_with_details(work_dir)?;
    let notice = (!details.is_empty()).then(|| Response::Error {
        message: format!("Orchestration: {}", details.join("; ")),
    });
    Ok(notice
        .into_iter()
        .chain([Response::OrchestrationComplete { summary }])
        .collect())
}

/// Forget the previous run's outcome. The outcome outlives its daemon so a failure
/// stays readable; a new run must not start out complete.
pub(crate) fn remove_previous_outcome(work_dir: &Path) -> Result<()> {
    for relative in [COMPLETION_MARKER, SCHEDULER_FAILURE_FILE] {
        super::storage::remove_control_file(work_dir, Path::new(relative))
            .with_context(|| format!("Failed to remove previous run's {relative}"))?;
    }
    Ok(())
}

fn collect_completion_with_details(work_dir: &Path) -> Result<(CompletionSummary, Vec<String>)> {
    let mut errors = Vec::new();
    let plan_path = completion_plan_path(work_dir).unwrap_or_else(|error| {
        errors.push(format!("Cannot read plan configuration: {error:#}"));
        "unknown".to_string()
    });
    let (stages, success_count, mut failure_count, total_duration_secs) =
        summarize_stages(completion_stages(&work_dir.join("stages"), &mut errors));
    failure_count += errors.len();
    if let Some(message) = outcome_diagnostic(read_terminal_outcome(work_dir)) {
        errors.push(message);
    }
    if !errors.is_empty() && failure_count == 0 {
        failure_count = 1;
    }
    if errors.is_empty() && failure_count > 0 {
        errors.push(format!(
            "{failure_count} stage(s) did not complete successfully"
        ));
    }
    Ok((
        CompletionSummary {
            total_duration_secs,
            stages,
            success_count,
            failure_count,
            plan_path,
        },
        errors,
    ))
}

fn summarize_stages(records: Vec<Stage>) -> (Vec<StageCompletionInfo>, usize, usize, i64) {
    let mut stages = Vec::new();
    let mut earliest_start = None;
    let mut latest_completion = None;
    let mut success_count = 0;
    let mut failure_count = 0;

    for stage in records {
        if let Some(start) = stage.started_at {
            earliest_start = Some(earliest_start.map_or(start, |previous| start.min(previous)));
        }
        if let Some(completed) = stage.completed_at {
            latest_completion =
                Some(latest_completion.map_or(completed, |previous| completed.max(previous)));
        }
        // The canonical bucket counts skipped stages as success and blocked,
        // merge-failure and review states as failure. A stopped in-flight stage
        // also makes the completion summary unsuccessful.
        match stage.status.bucket() {
            StatusBucket::Completed => success_count += 1,
            StatusBucket::Blocked | StatusBucket::Executing | StatusBucket::Pending => {
                failure_count += 1
            }
        }
        stages.push(completion_info(stage));
    }
    stages.sort_by(|a, b| a.id.cmp(&b.id));
    let total_duration_secs = earliest_start
        .zip(latest_completion)
        .map_or(0, |(start, end): (DateTime<Utc>, DateTime<Utc>)| {
            (end - start).num_seconds()
        });
    (stages, success_count, failure_count, total_duration_secs)
}

fn outcome_diagnostic(outcome: TerminalOutcome) -> Option<String> {
    match outcome {
        TerminalOutcome::Success => None,
        TerminalOutcome::SchedulerFailure(message) => Some(format!("Scheduler failure: {message}")),
        TerminalOutcome::FinalizationFailure(message) => {
            Some(format!("Finalization failure: {message}"))
        }
        TerminalOutcome::Incomplete(message) => {
            Some(format!("Orchestration incomplete: {message}"))
        }
    }
}

fn read_terminal_outcome(work_dir: &Path) -> TerminalOutcome {
    match fs::read_to_string(work_dir.join(COMPLETION_MARKER)) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|error| {
            TerminalOutcome::SchedulerFailure(format!("Cannot read terminal outcome: {error}"))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            TerminalOutcome::SchedulerFailure("terminal outcome is missing".into())
        }
        Err(error) => {
            TerminalOutcome::SchedulerFailure(format!("Cannot read terminal outcome: {error}"))
        }
    }
}

fn completion_plan_path(work_dir: &Path) -> Result<String> {
    let config_path = work_dir.join("config.toml");
    if !config_path.exists() {
        return Ok("unknown".to_string());
    }
    let config_content = fs::read_to_string(config_path)?;
    let config: toml::Value = toml::from_str(&config_content)?;
    Ok(config
        .get("plan")
        .and_then(|plan| plan.get("source_path"))
        .and_then(|source| source.as_str())
        .unwrap_or("unknown")
        .to_string())
}

fn completion_stages(stages_dir: &Path, errors: &mut Vec<String>) -> Vec<Stage> {
    let entries = match fs::read_dir(stages_dir) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("Cannot read stages directory: {error}"));
            return Vec::new();
        }
    };
    let mut stages = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!("Cannot read stage entry: {error}"));
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let stage = fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|content| parse_from_markdown::<Stage>(&content, "Stage"));
        match stage {
            Ok(stage) => stages.push(stage),
            Err(error) => errors.push(format!("Cannot read stage {}: {error:#}", path.display())),
        }
    }
    stages
}

fn completion_info(stage: Stage) -> StageCompletionInfo {
    let duration_secs = stage
        .started_at
        .zip(stage.completed_at)
        .map(|(start, completed)| (completed - start).num_seconds());
    StageCompletionInfo {
        id: stage.id,
        name: stage.name,
        status: stage.status,
        duration_secs,
        execution_secs: stage.execution_secs,
        retry_count: stage.retry_count,
        merged: stage.merged,
        dependencies: stage.dependencies,
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
