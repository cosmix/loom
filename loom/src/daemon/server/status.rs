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
    Ok(Response::StatusUpdate { data })
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
    let stages_dir = work_dir.join("stages");
    let config_path = work_dir.join("config.toml");

    // Read plan path from config.toml
    let plan_path = if config_path.exists() {
        let config_content = fs::read_to_string(&config_path)?;
        let config: toml::Value = toml::from_str(&config_content)?;
        config
            .get("plan")
            .and_then(|p| p.get("source_path"))
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        "unknown".to_string()
    };

    let mut stages: Vec<StageCompletionInfo> = Vec::new();
    let mut earliest_start: Option<DateTime<Utc>> = None;
    let mut latest_completion: Option<DateTime<Utc>> = None;
    let mut success_count = 0;
    let mut failure_count = 0;

    // Read all stage files
    if stages_dir.exists() {
        if let Ok(entries) = fs::read_dir(&stages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(stage) = parse_from_markdown::<Stage>(&content, "Stage") {
                            let started_at = stage.started_at.unwrap_or_else(chrono::Utc::now);
                            let completed_at = stage.completed_at;

                            // Track earliest start and latest completion
                            if earliest_start.is_none() || started_at < earliest_start.unwrap() {
                                earliest_start = Some(started_at);
                            }
                            if let Some(completed) = completed_at {
                                if latest_completion.is_none()
                                    || completed > latest_completion.unwrap()
                                {
                                    latest_completion = Some(completed);
                                }
                            }

                            // Count successes and failures via the canonical
                            // StageStatus::bucket() (D-5). Completed/Skipped count
                            // as success; all blocked/merge-failure/review states
                            // count as failure; in-flight (Executing/Pending) are
                            // ignored — a completion summary is produced once
                            // orchestration is terminal, so those are not expected.
                            match stage.status.bucket() {
                                StatusBucket::Completed => success_count += 1,
                                StatusBucket::Blocked => failure_count += 1,
                                StatusBucket::Executing | StatusBucket::Pending => {}
                            }

                            // Calculate duration if both timestamps exist
                            let duration_secs = completed_at
                                .map(|completed| (completed - started_at).num_seconds());

                            stages.push(StageCompletionInfo {
                                id: stage.id,
                                name: stage.name,
                                status: stage.status,
                                duration_secs,
                                execution_secs: stage.execution_secs,
                                retry_count: stage.retry_count,
                                merged: stage.merged,
                                dependencies: stage.dependencies,
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort stages by ID for consistent ordering
    stages.sort_by(|a, b| a.id.cmp(&b.id));

    // Calculate total duration
    let total_duration_secs = match (earliest_start, latest_completion) {
        (Some(start), Some(end)) => (end - start).num_seconds(),
        _ => 0,
    };

    Ok(CompletionSummary {
        total_duration_secs,
        stages,
        success_count,
        failure_count,
        plan_path,
    })
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
