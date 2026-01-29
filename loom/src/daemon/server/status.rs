//! Status collection and worktree status detection.

use super::super::protocol::{CompletionSummary, Response, StageCompletionInfo, StageInfo};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git::branch::branch_name_for_stage;
use crate::models::stage::StageStatus;
use crate::models::worktree::WorktreeStatus;
use crate::parser::frontmatter::extract_yaml_frontmatter;

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
                        if let Some(parsed) = parse_stage_frontmatter_full(&content) {
                            let session_pid =
                                get_session_pid(&sessions_dir, parsed.session.as_deref());
                            let started_at = get_stage_started_at(&content);
                            let completed_at = get_stage_completed_at(&content);
                            let worktree_status = detect_worktree_status(&parsed.id, &repo_root);

                            // Map status string to StageStatus enum
                            let status_enum = match parsed.status.as_str() {
                                "executing" => StageStatus::Executing,
                                "waiting-for-deps" | "pending" => StageStatus::WaitingForDeps,
                                "queued" | "ready" => StageStatus::Queued,
                                "completed" | "verified" => StageStatus::Completed,
                                "blocked" => StageStatus::Blocked,
                                "needs-handoff" => StageStatus::NeedsHandoff,
                                "waiting-for-input" => StageStatus::WaitingForInput,
                                "merge-conflict" => StageStatus::MergeConflict,
                                "completed-with-failures" => StageStatus::CompletedWithFailures,
                                "merge-blocked" => StageStatus::MergeBlocked,
                                "skipped" => StageStatus::Skipped,
                                _ => StageStatus::WaitingForDeps,
                            };

                            let stage_info = StageInfo {
                                id: parsed.id,
                                name: parsed.name,
                                session_pid,
                                started_at,
                                completed_at,
                                worktree_status,
                                status: status_enum.clone(),
                                merged: parsed.merged,
                                dependencies: parsed.dependencies,
                            };

                            // Categorize into lists based on status
                            match status_enum {
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
                                | StageStatus::MergeBlocked => {
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

/// Parsed stage data from frontmatter.
pub struct ParsedStage {
    pub id: String,
    pub name: String,
    pub status: String,
    pub session: Option<String>,
    pub merged: bool,
    pub dependencies: Vec<String>,
}

/// Parse stage frontmatter to extract id, name, status, and session.
///
/// Uses proper YAML parsing via serde_yaml for robustness. This handles
/// all YAML formats correctly (quoted strings, flow style, multiline values, etc.)
///
/// Parse stage frontmatter to extract all fields including merged and dependencies.
///
/// Uses proper YAML parsing via serde_yaml for robustness.
pub fn parse_stage_frontmatter_full(content: &str) -> Option<ParsedStage> {
    let yaml = extract_yaml_frontmatter(content).ok()?;

    // Extract required fields
    let id = yaml
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;

    let name = yaml
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;

    let status = yaml
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())?;

    // Extract optional session field
    let session = yaml
        .get("session")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && *s != "null" && *s != "~")
        .map(|s| s.to_string());

    // Extract merged field (defaults to false)
    let merged = yaml
        .get("merged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Extract dependencies array
    let dependencies = yaml
        .get("dependencies")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Some(ParsedStage {
        id,
        name,
        status,
        session,
        merged,
        dependencies,
    })
}

/// Get the started_at timestamp from stage content.
///
/// Extracts the `started_at` field from YAML frontmatter using proper parsing.
/// Falls back to `updated_at` for backward compatibility with older stage files.
pub fn get_stage_started_at(content: &str) -> chrono::DateTime<chrono::Utc> {
    // Use proper YAML parsing
    if let Ok(yaml) = extract_yaml_frontmatter(content) {
        // First try started_at (new field)
        if let Some(started_at) = yaml.get("started_at").and_then(|v| v.as_str()) {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(started_at) {
                return dt.with_timezone(&chrono::Utc);
            }
        }
        // Fall back to updated_at for backward compatibility
        if let Some(updated_at) = yaml.get("updated_at").and_then(|v| v.as_str()) {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(updated_at) {
                return dt.with_timezone(&chrono::Utc);
            }
        }
    }
    chrono::Utc::now()
}

/// Get stage completed_at timestamp from stage file content.
///
/// Extracts the `completed_at` field from YAML frontmatter using proper parsing.
pub fn get_stage_completed_at(content: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(yaml) = extract_yaml_frontmatter(content) {
        if let Some(completed_at) = yaml.get("completed_at").and_then(|v| v.as_str()) {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(completed_at) {
                return Some(dt.with_timezone(&chrono::Utc));
            }
        }
    }
    None
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
                        if let Some(parsed) = parse_stage_frontmatter_full(&content) {
                            let started_at = get_stage_started_at(&content);
                            let completed_at = get_stage_completed_at(&content);

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

                            // Map status string to enum
                            let status_enum = match parsed.status.as_str() {
                                "executing" => StageStatus::Executing,
                                "waiting-for-deps" | "pending" => StageStatus::WaitingForDeps,
                                "queued" | "ready" => StageStatus::Queued,
                                "completed" | "verified" => StageStatus::Completed,
                                "blocked" => StageStatus::Blocked,
                                "needs-handoff" => StageStatus::NeedsHandoff,
                                "waiting-for-input" => StageStatus::WaitingForInput,
                                "merge-conflict" => StageStatus::MergeConflict,
                                "completed-with-failures" => StageStatus::CompletedWithFailures,
                                "merge-blocked" => StageStatus::MergeBlocked,
                                "skipped" => StageStatus::Skipped,
                                _ => StageStatus::WaitingForDeps,
                            };

                            // Count successes and failures
                            match status_enum {
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
                                id: parsed.id,
                                name: parsed.name,
                                status: status_enum,
                                duration_secs,
                                merged: parsed.merged,
                                dependencies: parsed.dependencies,
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
