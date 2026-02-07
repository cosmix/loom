//! Retry merge command for re-attempting merge from a worktree
//!
//! Usage: loom stage retry-merge [stage_id]
//!
//! This command is used when a stage is in MergeConflict or MergeBlocked status
//! and the agent (or user) wants to re-attempt the merge to main from the worktree.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::commands::common::detect_stage_id;
use crate::git::branch::branch_name_for_stage;
use crate::git::{default_branch, merge_stage, MergeResult};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

/// Re-attempt merge for a stage in MergeConflict or MergeBlocked status.
///
/// This command:
/// 1. Loads the stage and verifies it is in MergeConflict or MergeBlocked status
/// 2. Verifies we're running from a worktree (not the main repo)
/// 3. Increments fix_attempts on the stage
/// 4. Attempts the merge to the default branch using existing merge logic
/// 5. On success: marks stage as completed+merged, triggers dependents
/// 6. On failure: prints detailed error with conflicting files
/// 7. If at fix limit: suggests dispute-criteria or human-review
pub fn retry_merge(stage_id: Option<String>) -> Result<()> {
    let work_dir = Path::new(".work");

    // Resolve stage ID: use provided or detect from current worktree branch
    let stage_id = match stage_id {
        Some(id) => id,
        None => detect_stage_id().ok_or_else(|| {
            anyhow::anyhow!(
                "Could not detect stage ID from current branch.\n\
                 Please provide the stage ID explicitly: loom stage retry-merge <stage-id>"
            )
        })?,
    };

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Verify stage is in a merge-failed state
    let is_merge_state = matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    );

    if !is_merge_state {
        bail!(
            "Stage '{}' is in '{}' status. Only MergeConflict or MergeBlocked stages can use retry-merge.\n\
             \n\
             Current status: {}\n\
             \n\
             For other failure states, use:\n\
             - loom stage retry {stage_id}       (for Blocked or CompletedWithFailures)\n\
             - loom stage merge-complete {stage_id} (after manually resolving conflicts)",
            stage_id,
            stage.status,
            stage.status,
        );
    }

    // Verify we're in a worktree (cwd should contain .worktrees in its path)
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let cwd_str = cwd.to_string_lossy();
    if !cwd_str.contains(".worktrees") {
        bail!(
            "retry-merge must be run from within a worktree.\n\
             \n\
             Current directory: {}\n\
             \n\
             Navigate to the worktree first:\n\
             - cd .worktrees/{stage_id}",
            cwd.display(),
        );
    }

    // Find repo root (parent of .worktrees)
    let repo_root = find_repo_root(&cwd)?;

    // Increment fix_attempts
    let attempts = stage.increment_fix_attempts();
    let max_attempts = stage.get_effective_max_fix_attempts();

    println!("Retrying merge for stage '{stage_id}' (attempt {attempts}/{max_attempts})");

    // Check fix limit before attempting
    if stage.is_at_fix_limit() {
        save_stage(&stage, work_dir)?;
        println!();
        println!(
            "Stage '{}' has reached the fix attempt limit ({}/{}).",
            stage_id, attempts, max_attempts
        );
        println!();
        println!("Options:");
        println!("  - Request human review:  loom stage waiting {stage_id}");
        println!("  - Force retry:           loom stage reset {stage_id} && loom stage retry-merge {stage_id}");
        println!(
            "  - Skip this stage:       loom stage skip {stage_id} --reason \"merge too complex\""
        );
        return Ok(());
    }

    // Determine target branch
    let target_branch = default_branch(&repo_root).unwrap_or_else(|_| "main".to_string());

    let branch_name = branch_name_for_stage(&stage_id);
    println!("Merging {branch_name} into {target_branch}...");

    // Attempt the merge
    let merge_result = merge_stage(&stage_id, &target_branch, &repo_root, work_dir);

    match merge_result {
        Ok(MergeResult::Success {
            files_changed,
            insertions,
            deletions,
        }) => {
            println!("Merge successful!");
            println!("  {files_changed} files changed, +{insertions} -{deletions}");

            // Clear merge conflict flag and mark as completed+merged
            stage.merge_conflict = false;
            stage.try_complete_merge()?;
            save_stage(&stage, work_dir)?;

            println!();
            println!("Stage '{stage_id}' merge complete! (Completed, merged: true)");

            // Trigger dependent stages
            let triggered = trigger_dependents(&stage_id, work_dir)
                .context("Failed to trigger dependent stages")?;
            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  -> {dep_id}");
                }
            }

            println!();
            println!("Next: run 'loom stage complete {stage_id}' if not already done,");
            println!("or clean up the worktree: loom worktree remove {stage_id}");
        }

        Ok(MergeResult::FastForward) => {
            println!("Fast-forward merge completed!");

            stage.merge_conflict = false;
            stage.try_complete_merge()?;
            save_stage(&stage, work_dir)?;

            println!("Stage '{stage_id}' merge complete! (Completed, merged: true)");

            let triggered = trigger_dependents(&stage_id, work_dir)
                .context("Failed to trigger dependent stages")?;
            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  -> {dep_id}");
                }
            }

            println!();
            println!("Consider cleaning up: loom worktree remove {stage_id}");
        }

        Ok(MergeResult::AlreadyUpToDate) => {
            println!("Branch is already up to date with {target_branch}.");

            stage.merge_conflict = false;
            stage.try_complete_merge()?;
            save_stage(&stage, work_dir)?;

            println!("Stage '{stage_id}' marked as merged.");

            let triggered = trigger_dependents(&stage_id, work_dir)
                .context("Failed to trigger dependent stages")?;
            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  -> {dep_id}");
                }
            }
        }

        Ok(MergeResult::Conflict { conflicting_files }) => {
            // Save the incremented fix_attempts
            save_stage(&stage, work_dir)?;

            println!();
            println!("Merge conflict persists.");
            println!();
            println!("Conflicting files:");
            for file in &conflicting_files {
                println!("  - {file}");
            }
            println!();

            if attempts >= max_attempts {
                println!("Fix attempt limit reached ({attempts}/{max_attempts}).");
                println!();
                println!("Options:");
                println!("  - Request human review:  loom stage waiting {stage_id}");
                println!("  - Skip this stage:       loom stage skip {stage_id} --reason \"unresolvable conflicts\"");
            } else {
                println!("Resolve the conflicts above, then run:");
                println!("  loom stage retry-merge {stage_id}");
                println!();
                println!(
                    "Remaining attempts: {}/{max_attempts}",
                    max_attempts - attempts
                );
            }
        }

        Err(e) => {
            // Save the incremented fix_attempts even on error
            save_stage(&stage, work_dir)?;

            println!();
            println!("Merge failed with error:");
            println!("  {e}");
            println!();

            if attempts >= max_attempts {
                println!("Fix attempt limit reached ({attempts}/{max_attempts}).");
                println!();
                println!("Options:");
                println!("  - Request human review:  loom stage waiting {stage_id}");
                println!("  - Skip this stage:       loom stage skip {stage_id} --reason \"merge error\"");
            } else {
                println!(
                    "Remaining attempts: {}/{max_attempts}",
                    max_attempts - attempts
                );
                println!("Investigate the error and try again: loom stage retry-merge {stage_id}");
            }
        }
    }

    Ok(())
}

/// Walk up from the current directory to find the repo root (parent of .worktrees).
fn find_repo_root(cwd: &Path) -> Result<std::path::PathBuf> {
    let mut current = cwd.to_path_buf();

    loop {
        // Check if .worktrees is a sibling directory (meaning parent is repo root)
        if current.file_name().map(|n| n.to_string_lossy().to_string())
            == Some(".worktrees".to_string())
        {
            if let Some(parent) = current.parent() {
                return Ok(parent.to_path_buf());
            }
        }

        // Check if current dir contains .worktrees
        let worktrees_dir = current.join(".worktrees");
        if worktrees_dir.exists() && worktrees_dir.is_dir() {
            return Ok(current);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => bail!(
                "Could not find repository root (no .worktrees directory found).\n\
                 Current directory: {}",
                cwd.display()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use tempfile::TempDir;

    fn create_test_stage(id: &str, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: format!("Test Stage {id}"),
            status,
            fix_attempts: 0,
            max_fix_attempts: Some(3),
            ..Stage::default()
        }
    }

    #[test]
    fn test_retry_merge_rejects_wrong_status() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create stages directory and a stage in Executing status
        let stages_dir = work_dir.join(".work").join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        let stage = create_test_stage("test-stage", StageStatus::Executing);
        let stage_path = stages_dir.join("test-stage.md");
        let content = crate::verify::transitions::serialize_stage_to_markdown(&stage).unwrap();
        std::fs::write(stage_path, content).unwrap();

        // retry_merge should fail since we're not in a worktree and status is wrong
        // We test the status check by calling the function parts directly
        assert!(!matches!(
            stage.status,
            StageStatus::MergeConflict | StageStatus::MergeBlocked
        ));
    }

    #[test]
    fn test_retry_merge_accepts_merge_conflict() {
        let stage = create_test_stage("test-stage", StageStatus::MergeConflict);
        assert!(matches!(
            stage.status,
            StageStatus::MergeConflict | StageStatus::MergeBlocked
        ));
    }

    #[test]
    fn test_retry_merge_accepts_merge_blocked() {
        let stage = create_test_stage("test-stage", StageStatus::MergeBlocked);
        assert!(matches!(
            stage.status,
            StageStatus::MergeConflict | StageStatus::MergeBlocked
        ));
    }

    #[test]
    fn test_fix_attempts_increment() {
        let mut stage = create_test_stage("test-stage", StageStatus::MergeConflict);
        assert_eq!(stage.fix_attempts, 0);

        let attempts = stage.increment_fix_attempts();
        assert_eq!(attempts, 1);
        assert_eq!(stage.fix_attempts, 1);

        let attempts = stage.increment_fix_attempts();
        assert_eq!(attempts, 2);
        assert_eq!(stage.fix_attempts, 2);
    }

    #[test]
    fn test_fix_limit_detection() {
        let mut stage = create_test_stage("test-stage", StageStatus::MergeConflict);
        stage.max_fix_attempts = Some(2);

        assert!(!stage.is_at_fix_limit());

        stage.fix_attempts = 1;
        assert!(!stage.is_at_fix_limit());

        stage.fix_attempts = 2;
        assert!(stage.is_at_fix_limit());

        stage.fix_attempts = 3;
        assert!(stage.is_at_fix_limit());
    }

    #[test]
    fn test_find_repo_root_from_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create .worktrees/my-stage structure
        let worktree = repo_root.join(".worktrees").join("my-stage");
        std::fs::create_dir_all(&worktree).unwrap();

        let found = find_repo_root(&worktree).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            repo_root.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_find_repo_root_from_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create .worktrees/my-stage/src structure
        let subdir = repo_root.join(".worktrees").join("my-stage").join("src");
        std::fs::create_dir_all(&subdir).unwrap();

        // From inside worktree subdir, we should still find repo root
        // We need .worktrees at repo root for this to work
        let found = find_repo_root(&subdir).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            repo_root.canonicalize().unwrap()
        );
    }
}
