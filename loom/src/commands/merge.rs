//! Merge completed stage worktree back to main
//! Usage: loom merge <stage_id> [--force]

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::fs::stage_files::find_stage_file;
use crate::models::stage::StageStatus;
use crate::orchestrator::session_is_running;
use crate::verify::transitions::{load_stage, transition_stage};

/// Find the tmux session name for a stage by checking session files
///
/// Looks for a session file in `.work/sessions/` that is assigned to the given stage
/// and returns its tmux_session name if found.
fn find_tmux_session_for_stage(stage_id: &str, work_dir: &Path) -> Result<Option<String>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(None);
    }

    let entries = std::fs::read_dir(&sessions_dir)
        .with_context(|| format!("Failed to read sessions directory: {}", sessions_dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse YAML frontmatter to check stage_id and get tmux_session
        if let Some(session_stage_id) = extract_frontmatter_field(&content, "stage_id") {
            if session_stage_id == stage_id {
                if let Some(tmux_session) = extract_frontmatter_field(&content, "tmux_session") {
                    return Ok(Some(tmux_session));
                }
            }
        }
    }

    Ok(None)
}

/// Extract a field value from YAML frontmatter
fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();

    // Check for frontmatter delimiter
    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        return None;
    }

    // Find end of frontmatter
    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim().starts_with("---") {
            end_idx = Some(idx);
            break;
        }
    }

    let end_idx = end_idx?;

    // Search for field in frontmatter
    for line in &lines[1..end_idx] {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == field {
                let value = value.trim();
                // Handle null values
                if value == "null" || value == "~" || value.is_empty() {
                    return None;
                }
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Validate that a stage is in an acceptable state for merging
fn validate_stage_status(stage_id: &str, work_dir: &Path, force: bool) -> Result<()> {
    let stages_dir = work_dir.join("stages");

    // If no stage file exists, skip validation (worktree without loom tracking)
    if find_stage_file(&stages_dir, stage_id)?.is_none() {
        return Ok(());
    }

    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage: {stage_id}"))?;

    let status_ok = matches!(stage.status, StageStatus::Completed | StageStatus::Verified);

    if !status_ok {
        if force {
            println!(
                "Warning: Stage '{}' is in '{:?}' status (not Completed/Verified). Proceeding due to --force.",
                stage_id, stage.status
            );
        } else {
            bail!(
                "Stage '{}' is in '{:?}' status. Only Completed or Verified stages can be merged.\n\
                 \n\
                 To mark the stage as complete, run:\n\
                   loom stage complete {}\n\
                 \n\
                 To force merge anyway (DANGEROUS - may lose work):\n\
                   loom merge {} --force",
                stage_id,
                stage.status,
                stage_id,
                stage_id
            );
        }
    }

    Ok(())
}

/// Check if there's an active tmux session for this stage
fn check_active_tmux_session(stage_id: &str, work_dir: &Path, force: bool) -> Result<()> {
    // First, check the standard naming convention: loom-{stage_id}
    let standard_tmux_name = format!("loom-{stage_id}");

    if session_is_running(&standard_tmux_name).unwrap_or(false) {
        if force {
            println!(
                "Warning: Stage '{stage_id}' has an active tmux session '{standard_tmux_name}'. Proceeding due to --force."
            );
        } else {
            bail!(
                "Stage '{stage_id}' has an active tmux session '{standard_tmux_name}'.\n\
                 \n\
                 The worktree may be in use by a running Claude Code session.\n\
                 \n\
                 To kill the session first:\n\
                   tmux kill-session -t {standard_tmux_name}\n\
                   # or\n\
                   loom sessions kill <session-id>\n\
                 \n\
                 To force merge anyway (DANGEROUS - will delete worktree from under active session):\n\
                   loom merge {stage_id} --force"
            );
        }
        return Ok(());
    }

    // Also check if there's a session file that references this stage with a different tmux name
    if let Some(tmux_name) = find_tmux_session_for_stage(stage_id, work_dir)? {
        if tmux_name != standard_tmux_name && session_is_running(&tmux_name).unwrap_or(false) {
            if force {
                println!(
                    "Warning: Stage '{stage_id}' has an active tmux session '{tmux_name}'. Proceeding due to --force."
                );
            } else {
                bail!(
                    "Stage '{stage_id}' has an active tmux session '{tmux_name}'.\n\
                     \n\
                     The worktree may be in use by a running Claude Code session.\n\
                     \n\
                     To kill the session first:\n\
                       tmux kill-session -t {tmux_name}\n\
                       # or\n\
                       loom sessions kill <session-id>\n\
                     \n\
                     To force merge anyway (DANGEROUS - will delete worktree from under active session):\n\
                       loom merge {stage_id} --force"
                );
            }
        }
    }

    Ok(())
}

/// Update stage status to Verified after successful merge
fn mark_stage_merged(stage_id: &str, work_dir: &Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");

    // Only update if stage file exists
    if find_stage_file(&stages_dir, stage_id)?.is_none() {
        // Stage file doesn't exist (might be a worktree without loom tracking)
        return Ok(());
    }

    // Transition to Verified status (if not already)
    let stage = load_stage(stage_id, work_dir)?;
    if stage.status != StageStatus::Verified {
        transition_stage(stage_id, StageStatus::Verified, work_dir)
            .with_context(|| format!("Failed to update stage status for: {stage_id}"))?;
        println!("Updated stage status to Verified");
    }

    Ok(())
}

/// Merge worktree branch to main, remove worktree on success
///
/// # Safety Checks (unless --force is used)
/// - Stage must be in Completed or Verified status
/// - No active tmux sessions for this stage
///
/// # Arguments
/// * `stage_id` - The ID of the stage to merge
/// * `force` - If true, skip safety checks (DANGEROUS)
pub fn execute(stage_id: String, force: bool) -> Result<()> {
    println!("Merging stage: {stage_id}");

    let repo_root = std::env::current_dir()?;
    let work_dir = repo_root.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    // Check worktree exists
    let worktree_path = repo_root.join(".worktrees").join(&stage_id);
    if !worktree_path.exists() {
        bail!(
            "Worktree for stage '{stage_id}' not found at {}",
            worktree_path.display()
        );
    }

    // Safety check 1: Validate stage status
    validate_stage_status(&stage_id, &work_dir, force)?;

    // Safety check 2: Check for active tmux sessions
    check_active_tmux_session(&stage_id, &work_dir, force)?;

    println!("Worktree path: {}", worktree_path.display());
    println!("Branch to merge: loom/{stage_id}");

    // Update stage status after successful merge validation
    mark_stage_merged(&stage_id, &work_dir).ok();

    println!("\nNote: Full merge requires Phase 4 (git module)");
    Ok(())
}

/// Get the worktree path for a stage
pub fn worktree_path(stage_id: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".worktrees")
        .join(stage_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_frontmatter_field() {
        let content = r#"---
id: session-123
stage_id: my-stage
tmux_session: loom-my-stage
status: running
---

# Session content
"#;

        assert_eq!(
            extract_frontmatter_field(content, "id"),
            Some("session-123".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "stage_id"),
            Some("my-stage".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "tmux_session"),
            Some("loom-my-stage".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "status"),
            Some("running".to_string())
        );
        assert_eq!(extract_frontmatter_field(content, "nonexistent"), None);
    }

    #[test]
    fn test_extract_frontmatter_field_null_values() {
        let content = r#"---
id: session-123
stage_id: null
tmux_session: ~
empty_field:
---
"#;

        assert_eq!(extract_frontmatter_field(content, "stage_id"), None);
        assert_eq!(extract_frontmatter_field(content, "tmux_session"), None);
        assert_eq!(extract_frontmatter_field(content, "empty_field"), None);
    }

    #[test]
    fn test_extract_frontmatter_field_no_frontmatter() {
        let content = "# Just a markdown file\nNo frontmatter here.";
        assert_eq!(extract_frontmatter_field(content, "id"), None);
    }

    #[test]
    fn test_find_tmux_session_for_stage_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let result = find_tmux_session_for_stage("stage-1", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_tmux_session_for_stage_found() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create sessions directory and a session file
        let sessions_dir = work_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let session_content = r#"---
id: session-abc-123
stage_id: my-target-stage
tmux_session: loom-session-abc
status: running
---

# Session details
"#;
        std::fs::write(sessions_dir.join("session-abc-123.md"), session_content).unwrap();

        let result = find_tmux_session_for_stage("my-target-stage", work_dir).unwrap();
        assert_eq!(result, Some("loom-session-abc".to_string()));

        // Different stage should not match
        let result = find_tmux_session_for_stage("other-stage", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_worktree_path() {
        let path = worktree_path("stage-1");
        assert!(path.to_string_lossy().contains(".worktrees"));
        assert!(path.to_string_lossy().contains("stage-1"));
    }
}
