//! Verification commands for loom.
//!
//! Provides commands to verify integrity of various loom resources,
//! particularly learning files that need protection from corruption.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use crate::fs::learnings::{verify_learnings, VerificationIssue, VerificationResult};

/// Execute the `loom verify learnings` command.
///
/// Compares current learning files against the pre-session snapshot to detect:
/// - Truncation (file got shorter)
/// - Deletion of learning files
/// - Removal of protected markers
///
/// If no snapshot exists, warns and returns success (can't verify without baseline).
pub fn learnings(session_id: Option<String>) -> Result<()> {
    let work_dir = find_work_dir()?;

    // If no session ID provided, try to detect from current worktree
    let session_id = match session_id {
        Some(id) => id,
        None => detect_session_id(&work_dir)?,
    };

    let result = verify_learnings(&work_dir, &session_id)
        .with_context(|| format!("Failed to verify learnings for session {session_id}"))?;

    match result {
        VerificationResult::Ok => {
            println!("Learnings verification passed for session {session_id}");
            Ok(())
        }
        VerificationResult::NoSnapshot => {
            eprintln!(
                "Warning: No snapshot exists for session {session_id}. \
                 Cannot verify without baseline."
            );
            // Return success since we can't verify without a baseline
            Ok(())
        }
        VerificationResult::Issues(issues) => {
            eprintln!("Learning file corruption detected for session {session_id}:");
            for issue in &issues {
                match issue {
                    VerificationIssue::Deleted(category) => {
                        eprintln!("  - {} file was deleted", category.filename());
                    }
                    VerificationIssue::Truncated {
                        category,
                        snapshot_len,
                        current_len,
                    } => {
                        eprintln!(
                            "  - {} was truncated: {} -> {} bytes",
                            category.filename(),
                            snapshot_len,
                            current_len
                        );
                    }
                    VerificationIssue::MarkerRemoved(category) => {
                        eprintln!("  - {} had protected marker removed", category.filename());
                    }
                }
            }
            anyhow::bail!(
                "Learning files were corrupted. Use `loom learn restore --session {session_id}` to restore from snapshot."
            );
        }
    }
}

/// Detect the current session ID from worktree context.
///
/// Looks for active session files in .work/sessions/ that match the current
/// worktree, or falls back to checking environment variables.
fn detect_session_id(work_dir: &std::path::Path) -> Result<String> {
    // First, try to get from environment (set by hooks)
    if let Ok(session_id) = std::env::var("LOOM_SESSION_ID") {
        return Ok(session_id);
    }

    // Try to detect from current worktree by looking at active sessions
    let sessions_dir = work_dir.join("sessions");
    if sessions_dir.exists() {
        // Get current working directory to determine which worktree we're in
        let cwd = std::env::current_dir()?;

        // Check if we're in a worktree by looking for .worktrees in the path
        if let Some(stage_id) = extract_stage_from_worktree_path(&cwd) {
            // Look for a session file that matches this stage
            for entry in std::fs::read_dir(&sessions_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    let content = std::fs::read_to_string(&path)?;
                    // Check if session is for this stage and is active
                    if content.contains(&format!("stage: {stage_id}"))
                        && content.contains("status: Active")
                    {
                        if let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) {
                            return Ok(session_id.to_string());
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!(
        "Could not detect session ID. Please provide --session <session-id> explicitly, \
         or set LOOM_SESSION_ID environment variable."
    )
}

/// Extract stage ID from a worktree path like `.worktrees/stage-name/...`
fn extract_stage_from_worktree_path(path: &std::path::Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    if let Some(idx) = path_str.find(".worktrees/") {
        let after_worktrees = &path_str[idx + ".worktrees/".len()..];
        // Take everything up to the next path separator
        let stage_id = after_worktrees.split(std::path::MAIN_SEPARATOR).next()?;
        if !stage_id.is_empty() {
            return Some(stage_id.to_string());
        }
    }
    None
}

/// Find the .work directory (current dir or parents)
fn find_work_dir() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let work_path = current.join(".work");
        if work_path.exists() && work_path.is_dir() {
            return Ok(work_path);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => bail!("Could not find .work directory. Are you in a loom workspace?"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::learnings::{
        append_learning, create_snapshot, init_learnings_dir, Learning, LearningCategory,
    };
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_stage_from_worktree_path() {
        use std::path::PathBuf;

        let path = PathBuf::from("/home/user/project/.worktrees/my-stage/src/main.rs");
        assert_eq!(
            extract_stage_from_worktree_path(&path),
            Some("my-stage".to_string())
        );

        let path = PathBuf::from("/home/user/project/src/main.rs");
        assert_eq!(extract_stage_from_worktree_path(&path), None);

        let path = PathBuf::from(".worktrees/test-stage");
        assert_eq!(
            extract_stage_from_worktree_path(&path),
            Some("test-stage".to_string())
        );
    }

    #[test]
    fn test_verify_learnings_passes_when_unchanged() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Initialize learnings
        init_learnings_dir(work_dir).unwrap();

        // Add a learning
        let learning = Learning {
            timestamp: Utc::now(),
            stage_id: "test-stage".to_string(),
            description: "Test learning".to_string(),
            correction: None,
            source: None,
        };
        append_learning(work_dir, LearningCategory::Mistake, &learning).unwrap();

        // Create snapshot
        create_snapshot(work_dir, "test-session").unwrap();

        // Verify should pass
        let result = verify_learnings(work_dir, "test-session").unwrap();
        assert!(matches!(result, VerificationResult::Ok));
    }

    #[test]
    fn test_verify_learnings_detects_truncation() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Initialize learnings
        init_learnings_dir(work_dir).unwrap();

        // Add a learning
        let learning = Learning {
            timestamp: Utc::now(),
            stage_id: "test-stage".to_string(),
            description: "Test learning with some content".to_string(),
            correction: Some("Important correction info".to_string()),
            source: None,
        };
        append_learning(work_dir, LearningCategory::Mistake, &learning).unwrap();

        // Create snapshot
        create_snapshot(work_dir, "test-session").unwrap();

        // Truncate the file
        let mistakes_file = work_dir.join("learnings").join("mistakes.md");
        fs::write(&mistakes_file, "truncated").unwrap();

        // Verify should detect truncation
        let result = verify_learnings(work_dir, "test-session").unwrap();
        match result {
            VerificationResult::Issues(issues) => {
                assert!(!issues.is_empty());
                let has_truncation = issues.iter().any(|i| {
                    matches!(
                        i,
                        VerificationIssue::Truncated {
                            category: LearningCategory::Mistake,
                            ..
                        }
                    )
                });
                assert!(has_truncation, "Expected truncation issue");
            }
            _ => panic!("Expected Issues result"),
        }
    }

    #[test]
    fn test_verify_learnings_no_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Initialize learnings without creating a snapshot
        init_learnings_dir(work_dir).unwrap();

        // Verify should return NoSnapshot
        let result = verify_learnings(work_dir, "nonexistent-session").unwrap();
        assert!(matches!(result, VerificationResult::NoSnapshot));
    }

    #[test]
    fn test_verify_learnings_detects_marker_removal() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Initialize learnings
        init_learnings_dir(work_dir).unwrap();

        // Create snapshot
        create_snapshot(work_dir, "test-session").unwrap();

        // Remove the protected marker
        let patterns_file = work_dir.join("learnings").join("patterns.md");
        fs::write(&patterns_file, "# Patterns\n\nNo marker here!").unwrap();

        // Verify should detect marker removal
        let result = verify_learnings(work_dir, "test-session").unwrap();
        match result {
            VerificationResult::Issues(issues) => {
                let has_marker_issue = issues.iter().any(|i| {
                    matches!(
                        i,
                        VerificationIssue::MarkerRemoved(LearningCategory::Pattern)
                    )
                });
                assert!(has_marker_issue, "Expected marker removed issue");
            }
            _ => panic!("Expected Issues result"),
        }
    }
}
