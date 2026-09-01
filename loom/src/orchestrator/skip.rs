//! Skip stage functionality
//!
//! This module provides functionality to skip stages that are blocked or waiting.
//! Skipped stages are marked as such and do not satisfy dependencies for downstream stages.

use anyhow::{bail, Result};
use std::path::Path;

use crate::models::stage::StageStatus;
use crate::verify::transitions::update_stage;

/// Skip a stage that is blocked, waiting for dependencies, or queued.
///
/// A skipped stage does not count as completed, so dependent stages will remain
/// in WaitingForDeps status. This is useful for explicitly bypassing stages that
/// are no longer needed or cannot be completed.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to skip
/// * `reason` - Optional reason for skipping the stage
/// * `work_dir` - Path to the `.loom/work` directory
///
/// # Returns
/// `Ok(())` on success
///
/// # Errors
/// Returns an error if:
/// - The stage cannot be loaded
/// - The stage is not in Blocked, WaitingForDeps, or Queued status
/// - The stage cannot be saved
///
/// # Example
/// ```no_run
/// use std::path::Path;
/// use loom::orchestrator::skip::skip_stage;
///
/// let work_dir = Path::new(".loom/work");
/// skip_stage("stage-1", Some("No longer needed".to_string()), work_dir)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn skip_stage(stage_id: &str, reason: Option<String>, work_dir: &Path) -> Result<()> {
    update_stage(stage_id, work_dir, |stage| {
        if !matches!(
            stage.status,
            StageStatus::Blocked | StageStatus::WaitingForDeps | StageStatus::Queued
        ) {
            bail!("Cannot skip stage in status: {}", stage.status);
        }
        stage.try_skip(reason)
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use crate::verify::transitions::{create_stage, load_stage};
    use tempfile::TempDir;

    fn create_test_stage(id: &str, name: &str, status: StageStatus) -> Stage {
        let mut stage = Stage::new(name.to_string(), Some(format!("Test stage {name}")));
        stage.id = id.to_string();
        stage.status = status;
        stage
    }

    #[test]
    fn test_skip_stage_from_blocked() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Blocked);
        create_stage(&stage, work_dir).expect("Should create stage");

        let result = skip_stage("stage-1", Some("Not needed".to_string()), work_dir);
        assert!(result.is_ok(), "Should skip blocked stage");

        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(reloaded.status, StageStatus::Skipped);
        assert_eq!(reloaded.close_reason, Some("Not needed".to_string()));
    }

    #[test]
    fn test_skip_stage_from_waiting_for_deps() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
        create_stage(&stage, work_dir).expect("Should create stage");

        let result = skip_stage("stage-1", Some("Dependency failed".to_string()), work_dir);
        assert!(result.is_ok(), "Should skip waiting stage");

        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(reloaded.status, StageStatus::Skipped);
        assert_eq!(reloaded.close_reason, Some("Dependency failed".to_string()));
    }

    #[test]
    fn test_skip_stage_from_queued() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Queued);
        create_stage(&stage, work_dir).expect("Should create stage");

        let result = skip_stage("stage-1", None, work_dir);
        assert!(result.is_ok(), "Should skip queued stage");

        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(reloaded.status, StageStatus::Skipped);
        assert_eq!(reloaded.close_reason, None);
    }

    #[test]
    fn test_skip_stage_from_executing_fails() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
        create_stage(&stage, work_dir).expect("Should create stage");

        let result = skip_stage("stage-1", Some("Should fail".to_string()), work_dir);
        assert!(result.is_err(), "Should not skip executing stage");

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Cannot skip stage in status"),
            "Error should mention invalid status: {err}"
        );
    }

    #[test]
    fn test_skip_stage_from_completed_fails() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Completed);
        create_stage(&stage, work_dir).expect("Should create stage");

        let result = skip_stage("stage-1", Some("Should fail".to_string()), work_dir);
        assert!(result.is_err(), "Should not skip completed stage");
    }

    #[test]
    fn test_skip_stage_updates_timestamp() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Blocked);
        create_stage(&stage, work_dir).expect("Should create stage");

        let original = load_stage("stage-1", work_dir).expect("Should load stage");
        let original_updated_at = original.updated_at;

        // Sleep to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        skip_stage("stage-1", Some("Test".to_string()), work_dir).expect("Should skip stage");

        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert!(
            reloaded.updated_at > original_updated_at,
            "updated_at should be newer after skip"
        );
    }
}
