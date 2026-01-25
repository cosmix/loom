//! Merge conflict resolution completion command
//!
//! This command is called after a user (or agent) resolves merge conflicts
//! for a stage in MergeConflict status. It verifies the merge is complete
//! and transitions the stage to Completed.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::git::get_conflicting_files;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

/// Complete merge conflict resolution for a stage.
///
/// This command:
/// 1. Verifies the stage is in MergeConflict status
/// 2. Checks that git working tree is clean (no unmerged files)
/// 3. Transitions stage to Completed with merged=true
/// 4. Triggers dependent stages
pub fn merge_complete(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Verify stage is in MergeConflict status
    if stage.status != StageStatus::MergeConflict {
        bail!(
            "Stage '{}' is not in MergeConflict status (current: {}). \
             Use this command only after merge conflicts have been resolved.",
            stage_id,
            stage.status
        );
    }

    // Check git status for unmerged files
    let repo_root = std::env::current_dir().context("Failed to get current directory")?;
    if !get_conflicting_files(&repo_root)?.is_empty() {
        bail!(
            "There are still unmerged files in the repository. \
             Please resolve all conflicts before running this command.\n\
             Run `git status` to see remaining conflicts."
        );
    }

    // Check if we're in the middle of a merge
    if is_merge_in_progress(&repo_root)? {
        bail!(
            "A merge is still in progress. \
             Please complete the merge with `git commit` before running this command."
        );
    }

    // Transition to Completed with merged=true
    stage.try_complete_merge()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' merge conflict resolution complete!");
    println!("  Status: Completed (merged: true)");

    // Trigger dependent stages
    let triggered =
        trigger_dependents(&stage_id, work_dir).context("Failed to trigger dependent stages")?;

    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  → {dep_id}");
        }
    }

    // Suggest cleanup
    println!();
    println!("Consider cleaning up the worktree:");
    println!("  loom worktree remove {stage_id}");

    Ok(())
}

/// Check if a merge is in progress
fn is_merge_in_progress(repo_root: &Path) -> Result<bool> {
    let merge_head = repo_root.join(".git").join("MERGE_HEAD");
    Ok(merge_head.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn test_get_conflicting_files_clean() {
        // In a clean repo, there should be no conflicting files
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Initialize a git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        assert!(get_conflicting_files(repo_root).unwrap().is_empty());
    }

    #[test]
    fn test_is_merge_in_progress_clean() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Initialize a git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        assert!(!is_merge_in_progress(repo_root).unwrap());
    }
}
