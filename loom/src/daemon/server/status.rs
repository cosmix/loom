//! Status collection and worktree status detection.

use super::super::protocol::{CompletionSummary, Response, StageCompletionInfo, StageInfo};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git::branch::branch_name_for_stage;
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
                            let worktree_status = detect_worktree_status(&stage.id, &repo_root);

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
                            };

                            // Categorize into lists based on status
                            match stage.status {
                                StageStatus::Executing => {
                                    stages_executing.push(stage_info);
                                }
                                StageStatus::WaitingForDeps | StageStatus::Queued => {
                                    stages_pending.push(stage_info);
                                }
                                StageStatus::Completed | StageStatus::Skipped => {
                                    stages_completed.push(stage_info);
                                }
                                StageStatus::Blocked
                                | StageStatus::NeedsHandoff
                                | StageStatus::WaitingForInput
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

/// Detect the worktree status for a stage.
///
/// Returns the appropriate WorktreeStatus based on:
/// - Whether the worktree directory exists
/// - Whether there are merge conflicts
/// - Whether a merge is in progress
/// - Whether the branch was manually merged outside of loom
pub fn detect_worktree_status(stage_id: &str, repo_root: &Path) -> Option<WorktreeStatus> {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);

    if !worktree_path.exists() {
        return None;
    }

    // Check for merge conflicts using git diff --name-only --diff-filter=U
    if has_merge_conflicts(&worktree_path) {
        return Some(WorktreeStatus::Conflict);
    }

    // Check if a merge is in progress by looking for MERGE_HEAD
    let merge_head = worktree_path.join(".git").join("MERGE_HEAD");
    // For worktrees, .git is a file pointing to the main repo, so check gitdir
    let git_path = worktree_path.join(".git");
    let is_merging = if git_path.is_file() {
        // Read gitdir path and check for MERGE_HEAD there
        if let Ok(content) = fs::read_to_string(&git_path) {
            if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                let gitdir_path = PathBuf::from(gitdir.trim());
                gitdir_path.join("MERGE_HEAD").exists()
            } else {
                false
            }
        } else {
            false
        }
    } else {
        merge_head.exists()
    };

    if is_merging {
        return Some(WorktreeStatus::Merging);
    }

    // Check if the branch was manually merged outside loom
    // This detects when users run `git merge loom/stage-id` manually
    if is_manually_merged(stage_id, repo_root) {
        return Some(WorktreeStatus::Merged);
    }

    Some(WorktreeStatus::Active)
}

/// Check if a loom branch has been manually merged into the default branch.
///
/// This is used to detect merges performed outside of loom (e.g., via CLI).
/// When detected, the orchestrator can trigger cleanup of the worktree.
pub fn is_manually_merged(stage_id: &str, repo_root: &Path) -> bool {
    use crate::git::{default_branch, is_branch_merged};

    // Get the default branch (main/master)
    let target_branch = match default_branch(repo_root) {
        Ok(branch) => branch,
        Err(_) => return false,
    };

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
