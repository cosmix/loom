//! Recovery logic for merge conflicts
//!
//! Handles detection of existing merge conflicts, finding active merge sessions,
//! and managing the recovery workflow for interrupted merges.

use anyhow::{Context, Result};
use std::path::Path;

use super::super::validation::extract_frontmatter_field;

/// Check if a merge conflict resolution session is already running for this stage
pub fn find_active_merge_session(stage_id: &str, work_dir: &Path) -> Result<Option<String>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(None);
    }

    // Look for session files that match merge session patterns
    for entry in std::fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Check if this session is for our stage and is a merge session
        if let Some(session_stage_id) = extract_frontmatter_field(&content, "stage_id") {
            if session_stage_id != stage_id {
                continue;
            }

            // Check if tmux session name indicates merge session
            if let Some(tmux_session) = extract_frontmatter_field(&content, "tmux_session") {
                if tmux_session.contains("merge")
                    && crate::orchestrator::session_is_running(&tmux_session).unwrap_or(false)
                {
                    return Ok(Some(tmux_session));
                }
            }

            // Check if PID indicates a running merge session
            // (for native backend, we check if the session ID contains "merge")
            if let Some(session_id) = extract_frontmatter_field(&content, "id") {
                if session_id.contains("merge") {
                    if let Some(pid_str) = extract_frontmatter_field(&content, "pid") {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            // Check if process is still alive
                            if std::process::Command::new("kill")
                                .arg("-0")
                                .arg(pid.to_string())
                                .output()
                                .map(|output| output.status.success())
                                .unwrap_or(false)
                            {
                                return Ok(Some(session_id));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Get current conflicting files in the repository
pub fn get_current_conflicts(repo_root: &Path) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to get conflicting files")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    Ok(files)
}
