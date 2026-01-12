//! Pre-merge validation and safety checks
//!
//! Contains functions for validating stage status and checking for active sessions
//! before allowing a merge operation to proceed.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::fs::stage_files::find_stage_file;
use crate::models::stage::StageStatus;
use crate::orchestrator::terminal::native::check_pid_alive;
use crate::verify::transitions::load_stage;

/// Find the session for a stage by checking session files
///
/// Looks for a session file in `.work/sessions/` that is assigned to the given stage
/// and returns its session ID if found.
pub fn find_session_for_stage(stage_id: &str, work_dir: &Path) -> Result<Option<String>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(None);
    }

    let entries = std::fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse YAML frontmatter to check stage_id and get session id
        if let Some(session_stage_id) = extract_frontmatter_field(&content, "stage_id") {
            if session_stage_id == stage_id {
                if let Some(session_id) = extract_frontmatter_field(&content, "id") {
                    return Ok(Some(session_id));
                }
            }
        }
    }

    Ok(None)
}

/// Extract a field value from YAML frontmatter
pub fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
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

/// Find active session for a stage, checking native backend
///
/// Returns session_id if an active session is found.
fn find_active_session_for_stage(stage_id: &str, work_dir: &Path) -> Result<Option<String>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(None);
    }

    let entries = std::fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Check if this session is assigned to our stage
        if let Some(session_stage_id) = extract_frontmatter_field(&content, "stage_id") {
            if session_stage_id != stage_id {
                continue;
            }

            // Check for native session (by PID)
            if let Some(pid_str) = extract_frontmatter_field(&content, "pid") {
                if let Ok(pid) = pid_str.parse::<u32>() {
                    if check_pid_alive(pid) {
                        // Return session ID as the identifier
                        let session_id = extract_frontmatter_field(&content, "id")
                            .unwrap_or_else(|| format!("pid-{pid}"));
                        return Ok(Some(session_id));
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Validate that a stage is in an acceptable state for merging
pub fn validate_stage_status(stage_id: &str, work_dir: &Path, force: bool) -> Result<()> {
    let stages_dir = work_dir.join("stages");

    // If no stage file exists, skip validation (worktree without loom tracking)
    if find_stage_file(&stages_dir, stage_id)?.is_none() {
        return Ok(());
    }

    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage: {stage_id}"))?;

    let status_ok = matches!(stage.status, StageStatus::Completed);

    if !status_ok {
        if force {
            println!(
                "Warning: Stage '{}' is in '{:?}' status (not Completed). Proceeding due to --force.",
                stage_id, stage.status
            );
        } else {
            bail!(
                "Stage '{}' is in '{:?}' status. Only Completed stages can be merged.\n\
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

/// Check if there's an active session for this stage (backend-aware)
///
/// This function checks for active sessions in the native backend.
/// Checks session files for PIDs and verifies they're still alive.
pub fn check_active_session(stage_id: &str, work_dir: &Path, force: bool) -> Result<()> {
    // Check for tracked sessions in .work/sessions/
    if let Some(session_id) = find_active_session_for_stage(stage_id, work_dir)? {
        if force {
            eprintln!(
                "Warning: Stage '{stage_id}' has an active native session ({session_id}). Proceeding due to --force."
            );
        } else {
            bail!(
                "Stage '{stage_id}' has an active native session ({session_id}).\n\
                 \n\
                 The worktree may be in use by a running Claude Code session.\n\
                 \n\
                 To complete the stage first:\n\
                   loom stage complete {stage_id}\n\
                 \n\
                 To kill the session:\n\
                   loom sessions kill {session_id}\n\
                 \n\
                 To force merge anyway (DANGEROUS - will delete worktree from under active session):\n\
                   loom merge {stage_id} --force"
            );
        }
    }

    Ok(())
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
pid: 12345
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
            extract_frontmatter_field(content, "pid"),
            Some("12345".to_string())
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
pid: ~
empty_field:
---
"#;

        assert_eq!(extract_frontmatter_field(content, "stage_id"), None);
        assert_eq!(extract_frontmatter_field(content, "pid"), None);
        assert_eq!(extract_frontmatter_field(content, "empty_field"), None);
    }

    #[test]
    fn test_extract_frontmatter_field_no_frontmatter() {
        let content = "# Just a markdown file\nNo frontmatter here.";
        assert_eq!(extract_frontmatter_field(content, "id"), None);
    }

    #[test]
    fn test_find_session_for_stage_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let result = find_session_for_stage("stage-1", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_session_for_stage_found() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create sessions directory and a session file
        let sessions_dir = work_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let session_content = r#"---
id: session-abc-123
stage_id: my-target-stage
pid: 12345
status: running
---

# Session details
"#;
        std::fs::write(sessions_dir.join("session-abc-123.md"), session_content).unwrap();

        let result = find_session_for_stage("my-target-stage", work_dir).unwrap();
        assert_eq!(result, Some("session-abc-123".to_string()));

        // Different stage should not match
        let result = find_session_for_stage("other-stage", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_active_session_for_stage_no_sessions() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let result = find_active_session_for_stage("stage-1", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_active_session_for_stage_native_dead_process() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create sessions directory
        let sessions_dir = work_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        // Create a session with a PID that doesn't exist (99999 is unlikely to exist)
        let session_content = r#"---
id: session-native-123
stage_id: my-stage
pid: 99999
status: running
---

# Native session
"#;
        std::fs::write(sessions_dir.join("session-native-123.md"), session_content).unwrap();

        // Should return None because the process is not alive
        let result = find_active_session_for_stage("my-stage", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_active_session_for_stage_native_current_process() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create sessions directory
        let sessions_dir = work_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        // Use the current process PID (guaranteed to be alive during the test)
        let current_pid = std::process::id();
        let session_content = format!(
            r#"---
id: session-native-test
stage_id: test-stage
pid: {current_pid}
status: running
---

# Native session with current PID
"#
        );
        std::fs::write(sessions_dir.join("session-native-test.md"), session_content).unwrap();

        // Should find the active session
        let result = find_active_session_for_stage("test-stage", work_dir).unwrap();
        assert!(result.is_some());
        let session_id = result.unwrap();
        assert_eq!(session_id, "session-native-test");
    }

    #[test]
    fn test_check_pid_alive_current_process() {
        let current_pid = std::process::id();
        assert!(check_pid_alive(current_pid));
    }

    #[test]
    fn test_check_pid_alive_nonexistent() {
        // PID 99999 is very unlikely to exist
        assert!(!check_pid_alive(99999));
    }
}
