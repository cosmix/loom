//! Status collection and worktree status detection.

use super::super::protocol::{CompletionSummary, Response, StageCompletionInfo, StageInfo};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git::branch::{branch_name_for_stage, resolve_target_branch};
use crate::models::stage::{Stage, StageStatus};
use crate::models::worktree::WorktreeStatus;
use crate::parser::frontmatter::{extract_yaml_frontmatter, parse_from_markdown};

/// Collect current stage status from the work directory.
pub fn collect_status(work_dir: &Path) -> Result<Response> {
    let stages_dir = work_dir.join("stages");
    let sessions_dir = work_dir.join("sessions");

    // Get repo root (parent of .work/)
    let repo_root = work_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut stages_executing = Vec::new();
    let mut stages_pending = Vec::new();
    let mut stages_completed = Vec::new();
    let mut stages_blocked = Vec::new();

    // Read stages directory
    if stages_dir.exists() {
        if let Ok(entries) = fs::read_dir(&stages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(stage) = parse_from_markdown::<Stage>(&content, "Stage") {
                            let session_pid =
                                get_session_pid(&sessions_dir, stage.session.as_deref());
                            let started_at = stage.started_at.unwrap_or_else(chrono::Utc::now);
                            let completed_at = stage.completed_at;
                            let worktree_status =
                                detect_worktree_status(&stage.id, &repo_root, work_dir);

                            let model = stage.effective_model().to_string();
                            let is_possibly_stuck = stage
                                .session
                                .as_deref()
                                .map(|sid| has_active_stuck_signal(work_dir, sid))
                                .unwrap_or(false);
                            let stage_info = StageInfo {
                                id: stage.id,
                                name: stage.name,
                                session_pid,
                                started_at,
                                completed_at,
                                worktree_status,
                                status: stage.status.clone(),
                                merged: stage.merged,
                                dependencies: stage.dependencies,
                                model,
                                is_possibly_stuck,
                            };

                            // Categorize into lists based on status.
                            // NeedsHandoff and WaitingForInput are active states where
                            // work is ongoing, so they belong in executing (matching CLI semantics).
                            match stage.status {
                                StageStatus::Executing
                                | StageStatus::NeedsHandoff
                                | StageStatus::WaitingForInput => {
                                    stages_executing.push(stage_info);
                                }
                                StageStatus::WaitingForDeps | StageStatus::Queued => {
                                    stages_pending.push(stage_info);
                                }
                                StageStatus::Completed | StageStatus::Skipped => {
                                    stages_completed.push(stage_info);
                                }
                                StageStatus::Blocked
                                | StageStatus::MergeConflict
                                | StageStatus::CompletedWithFailures
                                | StageStatus::MergeBlocked
                                | StageStatus::NeedsHumanReview => {
                                    stages_blocked.push(stage_info);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(Response::StatusUpdate {
        stages_executing,
        stages_pending,
        stages_completed,
        stages_blocked,
    })
}

/// Check whether the monitor has emitted an active `PossiblyStuck` soft signal
/// for the given session. Mirrors the logic in `commands::status::data::collector`
/// so static, compact, and live status modes render the same `[stuck?]` flag.
fn has_active_stuck_signal(work_dir: &Path, session_id: &str) -> bool {
    use crate::orchestrator::monitor::soft_signals::{read_active_for_session, SoftSignal};
    let now = std::time::SystemTime::now();
    read_active_for_session(work_dir, now, session_id)
        .unwrap_or_default()
        .into_iter()
        .any(|s| matches!(s, SoftSignal::PossiblyStuck { .. }))
}

/// Detect the worktree status for a stage.
///
/// Returns the appropriate WorktreeStatus based on:
/// - Whether the worktree directory exists
/// - Whether there are merge conflicts
/// - Whether a merge is in progress
/// - Whether the branch was manually merged outside of loom
pub fn detect_worktree_status(
    stage_id: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> Option<WorktreeStatus> {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);

    if !worktree_path.exists() {
        return None;
    }

    // Check for merge conflicts using git diff --name-only --diff-filter=U
    if has_merge_conflicts(&worktree_path) {
        return Some(WorktreeStatus::Conflict);
    }

    // Check if a merge is in progress (handles .git as directory or as file
    // with absolute/relative gitdir indirection).
    if crate::git::merge::merge_head_exists(&worktree_path).unwrap_or(false) {
        return Some(WorktreeStatus::Merging);
    }

    // Check if the branch was manually merged outside loom
    // This detects when users run `git merge loom/stage-id` manually
    if is_manually_merged(stage_id, repo_root, work_dir) {
        return Some(WorktreeStatus::Merged);
    }

    Some(WorktreeStatus::Active)
}

/// Check if a loom branch has been manually merged into the target branch.
///
/// This is used to detect merges performed outside of loom (e.g., via CLI).
/// When detected, the orchestrator can trigger cleanup of the worktree.
/// Uses `resolve_target_branch` to respect configured `base_branch` from config.toml.
pub fn is_manually_merged(stage_id: &str, repo_root: &Path, work_dir: &Path) -> bool {
    use crate::git::is_branch_merged;

    // Resolve target branch from config (respects base_branch setting)
    let base_branch = crate::fs::parse_base_branch_from_config(work_dir).unwrap_or(None);
    let target_branch = resolve_target_branch(&base_branch, repo_root);

    // Check if the loom branch has been merged into the target branch
    let branch_name = branch_name_for_stage(stage_id);
    is_branch_merged(&branch_name, &target_branch, repo_root).unwrap_or_default()
}

/// Check if there are unmerged paths (merge conflicts) in the worktree
pub fn has_merge_conflicts(worktree_path: &Path) -> bool {
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(worktree_path)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            !stdout.trim().is_empty()
        }
        Err(_) => false,
    }
}

/// Get session PID from session file.
///
/// Extracts the `pid` field from session YAML frontmatter using proper parsing.
pub fn get_session_pid(sessions_dir: &Path, session_id: Option<&str>) -> Option<u32> {
    let session_id = session_id?;

    // Try direct path first
    let session_path = sessions_dir.join(format!("{session_id}.md"));
    let content = if session_path.exists() {
        fs::read_to_string(&session_path).ok()?
    } else {
        // Search for matching file
        let entries = fs::read_dir(sessions_dir).ok()?;
        let mut found_content = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem == session_id || stem.contains(session_id) {
                    found_content = fs::read_to_string(&path).ok();
                    break;
                }
            }
        }
        found_content?
    };

    // Parse PID from frontmatter using proper YAML parsing
    let yaml = extract_yaml_frontmatter(&content).ok()?;
    yaml.get("pid")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
}

/// Collect completion summary from all stage files.
///
/// Gathers timing information and final status for all stages,
/// calculates total duration and success/failure counts.
///
/// # Arguments
/// * `work_dir` - The .work/ directory path
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

                            // Count successes and failures
                            match stage.status {
                                StageStatus::Completed | StageStatus::Skipped => {
                                    success_count += 1;
                                }
                                StageStatus::Blocked
                                | StageStatus::MergeConflict
                                | StageStatus::MergeBlocked
                                | StageStatus::CompletedWithFailures => {
                                    failure_count += 1;
                                }
                                _ => {}
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
    use crate::models::stage::Stage;
    use crate::verify::transitions::serialize_stage_to_markdown;

    fn write_stage_file(stages_dir: &Path, stage: &Stage) {
        let content = serialize_stage_to_markdown(stage).unwrap();
        std::fs::write(stages_dir.join(format!("{}.md", stage.id)), content).unwrap();
    }

    #[test]
    fn test_needs_handoff_categorized_as_executing() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let stages_dir = work_dir.join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        let mut stage = Stage::new("Test Handoff".to_string(), None);
        stage.id = "test-handoff".to_string();
        stage.status = StageStatus::NeedsHandoff;
        write_stage_file(&stages_dir, &stage);

        let response = collect_status(work_dir).unwrap();
        if let Response::StatusUpdate {
            stages_executing,
            stages_blocked,
            ..
        } = response
        {
            assert!(
                stages_executing.iter().any(|s| s.id == "test-handoff"),
                "NeedsHandoff should be in executing, not blocked"
            );
            assert!(
                !stages_blocked.iter().any(|s| s.id == "test-handoff"),
                "NeedsHandoff should NOT be in blocked"
            );
        } else {
            panic!("Expected StatusUpdate response");
        }
    }

    #[test]
    fn test_is_possibly_stuck_derived_from_soft_signals() {
        use crate::orchestrator::monitor::soft_signals::{append, SoftSignal, DECAY_WINDOW_SECS};

        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let stages_dir = work_dir.join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        let mut stage = Stage::new("Stuck Stage".to_string(), None);
        stage.id = "stuck-stage".to_string();
        stage.status = StageStatus::Executing;
        stage.session = Some("session-abc".to_string());
        write_stage_file(&stages_dir, &stage);

        // Before any soft signal: flag is false.
        let response = collect_status(work_dir).unwrap();
        if let Response::StatusUpdate {
            stages_executing, ..
        } = &response
        {
            let info = stages_executing
                .iter()
                .find(|s| s.id == "stuck-stage")
                .unwrap();
            assert!(
                !info.is_possibly_stuck,
                "is_possibly_stuck must be false until a soft signal is emitted"
            );
        } else {
            panic!("Expected StatusUpdate");
        }

        // Emit a PossiblyStuck soft signal matching the session.
        let now = chrono::Utc::now();
        let sig = SoftSignal::PossiblyStuck {
            session_id: "session-abc".to_string(),
            stage_id: "stuck-stage".to_string(),
            recent_events: 6,
            failure_count: 6,
            failure_ratio: 1.0,
            emitted_at: now.to_rfc3339(),
            expires_at: (now + chrono::Duration::seconds(DECAY_WINDOW_SECS as i64)).to_rfc3339(),
        };
        append(work_dir, &sig).unwrap();

        // After signal: flag is true.
        let response = collect_status(work_dir).unwrap();
        if let Response::StatusUpdate {
            stages_executing, ..
        } = response
        {
            let info = stages_executing
                .iter()
                .find(|s| s.id == "stuck-stage")
                .unwrap();
            assert!(
                info.is_possibly_stuck,
                "is_possibly_stuck must reflect the soft signal in the daemon path"
            );
        } else {
            panic!("Expected StatusUpdate");
        }
    }

    #[test]
    fn test_is_possibly_stuck_never_persisted_to_stage_yaml() {
        // The field must be derived at read time, not stored on disk. If a
        // future serde attribute regression caused it to be persisted, every
        // post-flag save would lie about the session being stuck even after
        // the signal expired.
        let mut stage = Stage::new("Field Persistence".to_string(), None);
        stage.is_possibly_stuck = true;
        let yaml = serialize_stage_to_markdown(&stage).unwrap();
        assert!(
            !yaml.contains("is_possibly_stuck"),
            "is_possibly_stuck must NOT appear in the serialized stage markdown; got:\n{yaml}"
        );
    }

    #[test]
    fn test_waiting_for_input_categorized_as_executing() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let stages_dir = work_dir.join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        let mut stage = Stage::new("Test Waiting".to_string(), None);
        stage.id = "test-waiting".to_string();
        stage.status = StageStatus::WaitingForInput;
        write_stage_file(&stages_dir, &stage);

        let response = collect_status(work_dir).unwrap();
        if let Response::StatusUpdate {
            stages_executing,
            stages_blocked,
            ..
        } = response
        {
            assert!(
                stages_executing.iter().any(|s| s.id == "test-waiting"),
                "WaitingForInput should be in executing"
            );
            assert!(
                !stages_blocked.iter().any(|s| s.id == "test-waiting"),
                "WaitingForInput should NOT be in blocked"
            );
        } else {
            panic!("Expected StatusUpdate response");
        }
    }
}
