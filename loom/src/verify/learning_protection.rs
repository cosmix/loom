//! Learning file protection verification.
//!
//! This module provides protection mechanisms for learning files to prevent
//! agents from accidentally or intentionally deleting accumulated learnings.
//!
//! Protection mechanisms:
//! 1. Pre-session snapshots of all learning files
//! 2. Post-session verification that files weren't truncated
//! 3. Automatic restoration from snapshots if issues detected
//! 4. Stop hook integration to block session exit if learnings damaged

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

use crate::fs::learnings::{
    cleanup_snapshot, create_snapshot, restore_from_snapshot, verify_learnings, VerificationIssue,
    VerificationResult,
};

/// Create a pre-session snapshot of learning files
///
/// This should be called when a session starts to capture the state
/// of all learning files before the agent runs.
pub fn snapshot_before_session(work_dir: &Path, session_id: &str) -> Result<()> {
    create_snapshot(work_dir, session_id)
        .with_context(|| format!("Failed to create learning snapshot for session {session_id}"))?;

    Ok(())
}

/// Verify and protect learning files after a session
///
/// Returns true if learnings are intact, false if restoration was needed.
/// This should be called from stop hooks or session completion.
pub fn verify_after_session(work_dir: &Path, session_id: &str) -> Result<ProtectionResult> {
    match verify_learnings(work_dir, session_id)? {
        VerificationResult::Ok => {
            // Clean up snapshot since verification passed
            cleanup_snapshot(work_dir, session_id)?;
            Ok(ProtectionResult::Intact)
        }
        VerificationResult::NoSnapshot => {
            // No snapshot to verify against - this is fine for new sessions
            Ok(ProtectionResult::NoSnapshot)
        }
        VerificationResult::Issues(issues) => {
            // Learning files were damaged - restore from snapshot
            eprintln!(
                "{} Learning file protection triggered!",
                "⚠".yellow().bold()
            );

            for issue in &issues {
                match issue {
                    VerificationIssue::Deleted(cat) => {
                        eprintln!("  {} {} was deleted", "✗".red(), cat.filename());
                    }
                    VerificationIssue::Truncated {
                        category,
                        snapshot_len,
                        current_len,
                    } => {
                        eprintln!(
                            "  {} {} was truncated ({} → {} bytes)",
                            "✗".red(),
                            category.filename(),
                            snapshot_len,
                            current_len
                        );
                    }
                    VerificationIssue::MarkerRemoved(cat) => {
                        eprintln!(
                            "  {} {} had protected marker removed",
                            "✗".red(),
                            cat.filename()
                        );
                    }
                }
            }

            // Restore from snapshot
            let restored = restore_from_snapshot(work_dir, session_id)?;

            eprintln!(
                "{} Restored {} learning file(s) from snapshot",
                "✓".green(),
                restored.len()
            );

            // Don't clean up snapshot - keep it for audit trail
            Ok(ProtectionResult::Restored {
                issues: issues.iter().map(format_issue).collect(),
                restored: restored.iter().map(|c| c.filename().to_string()).collect(),
            })
        }
    }
}

/// Format a verification issue for display
fn format_issue(issue: &VerificationIssue) -> String {
    match issue {
        VerificationIssue::Deleted(cat) => format!("{} deleted", cat.filename()),
        VerificationIssue::Truncated {
            category,
            snapshot_len,
            current_len,
        } => format!(
            "{} truncated ({} → {} bytes)",
            category.filename(),
            snapshot_len,
            current_len
        ),
        VerificationIssue::MarkerRemoved(cat) => {
            format!("{} marker removed", cat.filename())
        }
    }
}

/// Result of learning protection check
#[derive(Debug)]
pub enum ProtectionResult {
    /// All learning files are intact
    Intact,
    /// No snapshot existed (first session or manual)
    NoSnapshot,
    /// Issues were found and files were restored
    Restored {
        /// Description of issues found
        issues: Vec<String>,
        /// Files that were restored
        restored: Vec<String>,
    },
}

impl ProtectionResult {
    /// Whether the session should be blocked from completing
    pub fn should_warn(&self) -> bool {
        matches!(self, ProtectionResult::Restored { .. })
    }

    /// Get a summary message for display
    pub fn summary(&self) -> Option<String> {
        match self {
            ProtectionResult::Intact | ProtectionResult::NoSnapshot => None,
            ProtectionResult::Restored { issues, restored } => Some(format!(
                "Learning files were damaged ({}) and restored from snapshot ({})",
                issues.len(),
                restored.len()
            )),
        }
    }
}

/// Validate that a session hasn't damaged learning files
///
/// This is intended for use in stop hooks. Returns an error if
/// learning files were damaged (even though they were restored).
pub fn validate_for_hook(work_dir: &Path, session_id: &str) -> Result<()> {
    let result = verify_after_session(work_dir, session_id)?;

    if let ProtectionResult::Restored { issues, .. } = result {
        anyhow::bail!(
            "Learning files were damaged during session. Issues: {}. \
             Files have been restored from snapshot. Please review and re-commit.",
            issues.join(", ")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::learnings::{
        append_learning, category_file_path, init_learnings_dir, Learning, LearningCategory,
    };
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_env() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        init_learnings_dir(work_dir).unwrap();
        temp_dir
    }

    #[test]
    fn test_snapshot_and_verify_intact() {
        let temp_dir = setup_test_env();
        let work_dir = temp_dir.path();

        // Add a learning
        let learning = Learning {
            timestamp: Utc::now(),
            stage_id: "test".to_string(),
            description: "Test learning".to_string(),
            correction: None,
            source: None,
        };
        append_learning(work_dir, LearningCategory::Pattern, &learning).unwrap();

        // Snapshot
        snapshot_before_session(work_dir, "session-1").unwrap();

        // Verify (no changes)
        let result = verify_after_session(work_dir, "session-1").unwrap();
        assert!(matches!(result, ProtectionResult::Intact));
    }

    #[test]
    fn test_snapshot_and_verify_truncated() {
        let temp_dir = setup_test_env();
        let work_dir = temp_dir.path();

        // Add a learning
        let learning = Learning {
            timestamp: Utc::now(),
            stage_id: "test".to_string(),
            description: "Important learning that should not be lost".to_string(),
            correction: None,
            source: None,
        };
        append_learning(work_dir, LearningCategory::Pattern, &learning).unwrap();

        // Snapshot
        snapshot_before_session(work_dir, "session-2").unwrap();

        // Simulate truncation
        let pattern_file = category_file_path(work_dir, LearningCategory::Pattern);
        fs::write(&pattern_file, "truncated content").unwrap();

        // Verify - should detect and restore
        let result = verify_after_session(work_dir, "session-2").unwrap();
        assert!(matches!(result, ProtectionResult::Restored { .. }));

        // Verify content was restored
        let restored_content = fs::read_to_string(&pattern_file).unwrap();
        assert!(restored_content.contains("Important learning"));
    }

    #[test]
    fn test_validate_for_hook_fails_on_damage() {
        let temp_dir = setup_test_env();
        let work_dir = temp_dir.path();

        let learning = Learning {
            timestamp: Utc::now(),
            stage_id: "test".to_string(),
            description: "Test".to_string(),
            correction: None,
            source: None,
        };
        append_learning(work_dir, LearningCategory::Mistake, &learning).unwrap();

        snapshot_before_session(work_dir, "session-3").unwrap();

        // Delete the file
        let mistake_file = category_file_path(work_dir, LearningCategory::Mistake);
        fs::remove_file(&mistake_file).unwrap();

        // Hook validation should fail
        let result = validate_for_hook(work_dir, "session-3");
        assert!(result.is_err());
    }
}
