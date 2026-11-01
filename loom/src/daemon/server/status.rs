//! Status collection and completion summaries.

use super::super::protocol::{CompletionSummary, Response, StageCompletionInfo};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

use crate::models::stage::{Stage, StatusBucket};
use crate::parser::frontmatter::parse_from_markdown;

/// Collect current stage status from the work directory.
pub fn collect_status(work_dir: &Path) -> Result<Response> {
    let wd = crate::fs::work_dir::WorkDir::new(work_dir)
        .with_context(|| format!("Failed to resolve work dir: {}", work_dir.display()))?;
    let data = crate::commands::status::data::collect_status_data(&wd)?;
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
    let plan_path = completion_plan_path(work_dir)?;
    let mut stages = Vec::new();
    let mut earliest_start = None;
    let mut latest_completion = None;
    let mut success_count = 0;
    let mut failure_count = 0;

    for stage in completion_stages(&work_dir.join("stages")) {
        if let Some(start) = stage.started_at {
            earliest_start = Some(earliest_start.map_or(start, |previous| start.min(previous)));
        }
        if let Some(completed) = stage.completed_at {
            latest_completion =
                Some(latest_completion.map_or(completed, |previous| completed.max(previous)));
        }
        // The canonical bucket counts skipped stages as success and blocked,
        // merge-failure and review states as failure. In-flight stages do not count.
        match stage.status.bucket() {
            StatusBucket::Completed => success_count += 1,
            StatusBucket::Blocked => failure_count += 1,
            StatusBucket::Executing | StatusBucket::Pending => {}
        }
        stages.push(completion_info(stage));
    }
    stages.sort_by(|a, b| a.id.cmp(&b.id));
    let total_duration_secs = earliest_start
        .zip(latest_completion)
        .map_or(0, |(start, end): (DateTime<Utc>, DateTime<Utc>)| {
            (end - start).num_seconds()
        });
    Ok(CompletionSummary {
        total_duration_secs,
        stages,
        success_count,
        failure_count,
        plan_path,
    })
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

fn completion_stages(stages_dir: &Path) -> Vec<Stage> {
    let Ok(entries) = fs::read_dir(stages_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("md"))
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|content| parse_from_markdown::<Stage>(&content, "Stage").ok())
        .collect()
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
mod tests {
    use super::*;
    use crate::daemon::protocol::Response;
    use crate::models::stage::{Stage, StageStatus};
    use crate::verify::transitions::serialize_stage_to_markdown;

    fn write_stage_file(stages_dir: &Path, stage: &Stage) {
        let content = serialize_stage_to_markdown(stage).unwrap();
        std::fs::write(stages_dir.join(format!("{}.md", stage.id)), content).unwrap();
    }

    #[test]
    fn completion_durations_require_real_starts() {
        let temp = tempfile::tempdir().unwrap();
        let stages_dir = temp.path().join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();
        let time = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);

        let mut normal = Stage::new("Normal".to_string(), None);
        normal.id = "normal".to_string();
        normal.status = StageStatus::Completed;
        normal.started_at = Some(time("2026-01-01T00:00:00Z"));
        normal.completed_at = Some(time("2026-01-01T00:01:00Z"));
        write_stage_file(&stages_dir, &normal);

        let mut skipped = Stage::new("Skipped".to_string(), None);
        skipped.id = "skipped".to_string();
        skipped.status = StageStatus::Skipped;
        skipped.completed_at = Some(time("2026-01-01T00:02:00Z"));
        write_stage_file(&stages_dir, &skipped);

        let summary = collect_completion_summary(temp.path()).unwrap();
        assert_eq!(summary.stages[0].duration_secs, Some(60));
        assert_eq!(summary.stages[1].duration_secs, None);
        assert_eq!(summary.total_duration_secs, 120);

        std::fs::remove_file(stages_dir.join("normal.md")).unwrap();
        let summary = collect_completion_summary(temp.path()).unwrap();
        assert_eq!(summary.stages[0].duration_secs, None);
        assert_eq!(summary.total_duration_secs, 0);
    }

    #[test]
    fn collect_status_returns_status_data() {
        let temp = tempfile::tempdir().unwrap();
        let wd = crate::fs::work_dir::WorkDir::new(temp.path().join(".loom/work")).unwrap();
        let stages_dir = wd.root().join("stages");
        std::fs::create_dir_all(stages_dir).unwrap();

        let mut stage = Stage::new("Test Waiting".to_string(), None);
        stage.id = "test-waiting".to_string();
        stage.status = StageStatus::WaitingForInput;
        write_stage_file(&wd.root().join("stages"), &stage);

        let response = collect_status(wd.root()).unwrap();
        match response {
            Response::StatusUpdate { data } => {
                let stage = data
                    .stages
                    .iter()
                    .find(|row| row.id == "test-waiting")
                    .unwrap();
                assert_eq!(stage.status, StageStatus::WaitingForInput);
            }
            _ => panic!("Expected StatusUpdate response"),
        }
    }
}
