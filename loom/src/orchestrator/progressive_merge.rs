//! Progressive merge service for immediate branch merging after verification
//!
//! This module provides functionality to merge stage branches immediately after
//! verification passes. This is the core of conflict prevention - by merging
//! verified branches as soon as they pass, we minimize the window for conflicts.
//!
//! The merge uses file-based locking to prevent concurrent merges from multiple
//! stages completing simultaneously.

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crate::git::branch::branch_exists;
use crate::git::merge::{merge_stage, MergeResult};
use crate::models::stage::Stage;

// Merge lock timeout for detecting stale locks (5 minutes)
const MERGE_LOCK_STALE_TIMEOUT_SECS: u64 = 300;

/// Result of a progressive merge attempt
#[derive(Debug, Clone)]
pub enum ProgressiveMergeResult {
    /// Merge completed successfully with changes
    Success { files_changed: u32 },
    /// Fast-forward merge completed (no merge commit needed)
    FastForward,
    /// Branch was already merged or up to date
    AlreadyMerged,
    /// Conflicts detected that need resolution
    Conflict { conflicting_files: Vec<String> },
    /// Branch doesn't exist (already cleaned up)
    NoBranch,
}

impl ProgressiveMergeResult {
    /// Returns true if the merge succeeded (no conflicts)
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            ProgressiveMergeResult::Success { .. }
                | ProgressiveMergeResult::FastForward
                | ProgressiveMergeResult::AlreadyMerged
                | ProgressiveMergeResult::NoBranch
        )
    }

    /// Returns the conflicting files if there was a conflict
    pub fn conflicting_files(&self) -> Option<&[String]> {
        match self {
            ProgressiveMergeResult::Conflict { conflicting_files } => Some(conflicting_files),
            _ => None,
        }
    }
}

/// File-based merge lock to prevent concurrent merges
pub struct MergeLock {
    lock_path: std::path::PathBuf,
    held: bool,
}

impl MergeLock {
    /// Acquire the merge lock with a timeout
    ///
    /// The lock is a simple file-based lock stored at `.work/merge.lock`.
    /// If another process holds the lock, this will wait up to `timeout`
    /// before failing.
    pub fn acquire(work_dir: &Path, timeout: Duration) -> Result<Self> {
        let lock_path = work_dir.join("merge.lock");
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(100);

        loop {
            match Self::try_acquire(&lock_path) {
                Ok(lock) => return Ok(lock),
                Err(_) if start.elapsed() < timeout => {
                    std::thread::sleep(poll_interval);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Try to acquire the lock without waiting
    fn try_acquire(lock_path: &Path) -> Result<Self> {
        // Try to create the lock file exclusively
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                // Write our PID and timestamp to the lock file
                let pid = std::process::id();
                let timestamp = chrono::Utc::now().to_rfc3339();
                writeln!(file, "pid={pid}")?;
                writeln!(file, "timestamp={timestamp}")?;
                file.sync_all()?;
                Ok(Self {
                    lock_path: lock_path.to_path_buf(),
                    held: true,
                })
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                // Lock is held by another process - check if it's stale
                if Self::is_lock_stale(lock_path)? {
                    // Remove stale lock and retry
                    fs::remove_file(lock_path).ok();
                    Self::try_acquire(lock_path)
                } else {
                    Err(anyhow::anyhow!("Merge lock is held by another process"))
                }
            }
            Err(e) => Err(e).context("Failed to acquire merge lock"),
        }
    }

    /// Check if an existing lock is stale (older than 5 minutes)
    fn is_lock_stale(lock_path: &Path) -> Result<bool> {
        let metadata = fs::metadata(lock_path)?;
        let modified = metadata.modified()?;
        let age = std::time::SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::ZERO);

        // Consider lock stale if older than MERGE_LOCK_STALE_TIMEOUT_SECS
        Ok(age > Duration::from_secs(MERGE_LOCK_STALE_TIMEOUT_SECS))
    }

    /// Release the lock
    pub fn release(mut self) -> Result<()> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<()> {
        if self.held {
            fs::remove_file(&self.lock_path).ok();
            self.held = false;
        }
        Ok(())
    }
}

impl Drop for MergeLock {
    fn drop(&mut self) {
        // Best-effort release on drop
        self.release_inner().ok();
    }
}

/// Attempt to merge a just-completed stage into the merge point.
///
/// Called immediately after verification passes. This function:
/// 1. Acquires a file-based lock to prevent concurrent merges
/// 2. Checks if the stage's branch exists
/// 3. Attempts to merge the branch into the merge point
/// 4. Returns the result (success, conflict, or no-op)
///
/// # Arguments
/// * `stage` - The stage that just completed verification
/// * `repo_root` - Path to the repository root
/// * `merge_point` - Target branch to merge into (usually "main" or a staging branch)
///
/// # Returns
/// * `Ok(ProgressiveMergeResult::Success)` - Branch merged successfully
/// * `Ok(ProgressiveMergeResult::FastForward)` - Fast-forward merge completed
/// * `Ok(ProgressiveMergeResult::AlreadyMerged)` - No changes to merge
/// * `Ok(ProgressiveMergeResult::Conflict)` - Conflicts detected, stage needs resolution
/// * `Ok(ProgressiveMergeResult::NoBranch)` - Branch doesn't exist (already cleaned up)
/// * `Err(_)` - Unexpected error during merge
pub fn merge_completed_stage(
    stage: &Stage,
    repo_root: &Path,
    merge_point: &str,
) -> Result<ProgressiveMergeResult> {
    merge_completed_stage_with_timeout(stage, repo_root, merge_point, Duration::from_secs(30))
}

/// Attempt to merge with a custom lock timeout
pub fn merge_completed_stage_with_timeout(
    stage: &Stage,
    repo_root: &Path,
    merge_point: &str,
    lock_timeout: Duration,
) -> Result<ProgressiveMergeResult> {
    let branch_name = format!("loom/{}", stage.id);

    // Check if branch exists before trying to merge
    if !branch_exists(&branch_name, repo_root)? {
        return Ok(ProgressiveMergeResult::NoBranch);
    }

    // Get the work directory for locking
    let work_dir = repo_root.join(".work");
    if !work_dir.exists() {
        return Err(anyhow::anyhow!(".work directory not found"));
    }

    // Acquire merge lock to prevent concurrent merges
    let _lock = MergeLock::acquire(&work_dir, lock_timeout)
        .context("Failed to acquire merge lock - another merge may be in progress")?;

    // Attempt the merge
    let result = merge_stage(&stage.id, merge_point, repo_root)
        .with_context(|| format!("Failed to merge stage {} into {}", stage.id, merge_point))?;

    // Convert git::merge::MergeResult to ProgressiveMergeResult
    let progressive_result = match result {
        MergeResult::Success { files_changed, .. } => {
            ProgressiveMergeResult::Success { files_changed }
        }
        MergeResult::FastForward => ProgressiveMergeResult::FastForward,
        MergeResult::AlreadyUpToDate => ProgressiveMergeResult::AlreadyMerged,
        MergeResult::Conflict { conflicting_files } => {
            ProgressiveMergeResult::Conflict { conflicting_files }
        }
    };

    // Lock is automatically released when _lock goes out of scope
    Ok(progressive_result)
}

/// Parse the merge point (base_branch) from config.toml
///
/// Falls back to "main" if not configured.
pub fn get_merge_point(work_dir: &Path) -> Result<String> {
    let config_path = work_dir.join("config.toml");

    if !config_path.exists() {
        return Ok("main".to_string());
    }

    let config_content = fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    let base_branch = config
        .get("plan")
        .and_then(|p| p.get("base_branch"))
        .and_then(|b| b.as_str())
        .map(String::from)
        .unwrap_or_else(|| "main".to_string());

    Ok(base_branch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_stage(id: &str) -> Stage {
        Stage {
            id: id.to_string(),
            name: format!("Test Stage {id}"),
            description: None,
            status: StageStatus::Completed,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            stage_type: crate::models::stage::StageType::default(),
            plan_id: None,
            worktree: Some(id.to_string()),
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: Some(Utc::now()),
            close_reason: None,
            auto_merge: None,
            working_dir: Some(".".to_string()),
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
        }
    }

    #[test]
    fn test_progressive_merge_result_is_success() {
        assert!(ProgressiveMergeResult::Success { files_changed: 5 }.is_success());
        assert!(ProgressiveMergeResult::FastForward.is_success());
        assert!(ProgressiveMergeResult::AlreadyMerged.is_success());
        assert!(ProgressiveMergeResult::NoBranch.is_success());
        assert!(!ProgressiveMergeResult::Conflict {
            conflicting_files: vec!["file.rs".to_string()]
        }
        .is_success());
    }

    #[test]
    fn test_progressive_merge_result_conflicting_files() {
        let conflict = ProgressiveMergeResult::Conflict {
            conflicting_files: vec!["a.rs".to_string(), "b.rs".to_string()],
        };
        assert_eq!(
            conflict.conflicting_files(),
            Some(&["a.rs".to_string(), "b.rs".to_string()][..])
        );

        assert!(ProgressiveMergeResult::Success { files_changed: 1 }
            .conflicting_files()
            .is_none());
    }

    #[test]
    fn test_merge_lock_acquire_release() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // First acquire should succeed
        let lock = MergeLock::acquire(work_dir, Duration::from_secs(1)).unwrap();
        assert!(work_dir.join("merge.lock").exists());

        // Release should remove the lock file
        lock.release().unwrap();
        assert!(!work_dir.join("merge.lock").exists());
    }

    #[test]
    fn test_merge_lock_concurrent_fails() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // First acquire should succeed
        let _lock1 = MergeLock::acquire(work_dir, Duration::from_secs(1)).unwrap();

        // Second acquire should fail (with short timeout)
        let result = MergeLock::acquire(work_dir, Duration::from_millis(100));
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_lock_drop_releases() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        {
            let _lock = MergeLock::acquire(work_dir, Duration::from_secs(1)).unwrap();
            assert!(work_dir.join("merge.lock").exists());
        }

        // Lock should be released on drop
        assert!(!work_dir.join("merge.lock").exists());
    }

    #[test]
    fn test_get_merge_point_default() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // No config.toml - should return "main"
        let result = get_merge_point(work_dir).unwrap();
        assert_eq!(result, "main");
    }

    #[test]
    fn test_get_merge_point_from_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create config.toml with custom base_branch
        let config_content = r#"
[plan]
source_path = "doc/plans/test.md"
plan_id = "test"
base_branch = "develop"
"#;
        fs::write(work_dir.join("config.toml"), config_content).unwrap();

        let result = get_merge_point(work_dir).unwrap();
        assert_eq!(result, "develop");
    }

    #[test]
    fn test_get_merge_point_missing_base_branch() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create config.toml without base_branch
        let config_content = r#"
[plan]
source_path = "doc/plans/test.md"
plan_id = "test"
"#;
        fs::write(work_dir.join("config.toml"), config_content).unwrap();

        let result = get_merge_point(work_dir).unwrap();
        assert_eq!(result, "main");
    }

    #[test]
    fn test_stage_fields_exist() {
        // Verify the Stage model has the merge tracking fields we need
        let mut stage = create_test_stage("test");
        assert!(!stage.merged);
        assert!(!stage.merge_conflict);

        // Should be able to update these fields
        stage.merged = true;
        stage.merge_conflict = true;
        assert!(stage.merged);
        assert!(stage.merge_conflict);
    }
}
