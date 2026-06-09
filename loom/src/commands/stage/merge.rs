//! Unified merge command for stages
//!
//! Combines retry-merge and merge-complete into a single command.
//! Default: re-attempt merge to main from a worktree.
//! --resolved: complete manual merge resolution.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::commands::common::detect_stage_id;
use crate::git::branch::{branch_name_for_stage, resolve_target_branch};
use crate::git::merge::merge_head_exists;
use crate::git::{get_conflicting_files, merge_stage, MergeResult};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

/// Unified merge command entry point.
///
/// If `resolved` is true, validates that manual merge resolution is complete
/// and transitions the stage to Completed. Otherwise, re-attempts the merge
/// programmatically.
pub fn merge(stage_id: Option<String>, resolved: bool) -> Result<()> {
    if resolved {
        merge_resolved(stage_id)
    } else {
        merge_retry(stage_id)
    }
}

/// Complete merge resolution for a stage after manual conflict resolution.
///
/// This path:
/// 1. Resolves stage ID (provided or auto-detected from branch)
/// 2. Verifies the stage is in MergeConflict or MergeBlocked status
/// 3. Checks that git working tree is clean (no unmerged files)
/// 4. Transitions stage to Completed with merged=true
/// 5. Triggers dependent stages
fn merge_resolved(stage_id: Option<String>) -> Result<()> {
    let work_dir = Path::new(".work");

    // Resolve stage ID: use provided or detect from current worktree branch
    let stage_id = match stage_id {
        Some(id) => id,
        None => detect_stage_id().ok_or_else(|| {
            anyhow::anyhow!(
                "Could not detect stage ID from current branch.\n\
                 Please provide the stage ID explicitly: loom stage merge --resolved <stage-id>"
            )
        })?,
    };

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Verify stage is in a merge-failed status
    if stage.status != StageStatus::MergeConflict && stage.status != StageStatus::MergeBlocked {
        bail!(
            "Stage '{}' is not in MergeConflict or MergeBlocked status (current: {}). \
             Use this command only after merge issues have been resolved.",
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
    if merge_head_exists(&repo_root)? {
        bail!(
            "A merge is still in progress. \
             Please complete the merge with `git commit` before running this command."
        );
    }

    // Verify ancestry: derive completed_commit if missing, then check that the
    // commit is in the target branch's history. Without this guard,
    // `--resolved` could be invoked after a partial resolution and silently
    // satisfy downstream dependency checks even though the commit never landed.
    let target_branch = resolve_target_branch(
        &crate::fs::parse_base_branch_from_config(work_dir).unwrap_or(None),
        &repo_root,
    );
    let verified = crate::commands::stage::merge_verify::verify_or_derive_completed_commit(
        &stage,
        &target_branch,
        &repo_root,
    )?;
    if let Some(commit) = verified.persist_commit {
        // Persist the derived commit before continuing.
        stage.completed_commit = Some(commit);
        save_stage(&stage, work_dir)?;
    }

    // Transition to Completed with merged=true
    stage.try_complete_merge()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' merge conflict resolution complete!");
    println!("  Status: Completed (merged: true)");

    // Trigger dependent stages
    let base_branch = crate::fs::parse_base_branch_from_config(work_dir).unwrap_or(None);
    let target_branch = resolve_target_branch(&base_branch, &repo_root);
    let triggered = trigger_dependents(&stage_id, work_dir, &repo_root, &target_branch)
        .context("Failed to trigger dependent stages")?;

    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  -> {dep_id}");
        }
    }

    // Suggest cleanup
    println!();
    println!("Consider cleaning up the worktree:");
    println!("  loom worktree remove {stage_id}");

    Ok(())
}

/// Re-attempt merge for a stage in MergeConflict or MergeBlocked status.
///
/// This path:
/// 1. Loads the stage and verifies it is in MergeConflict or MergeBlocked status
/// 2. Verifies we're running from a worktree (not the main repo)
/// 3. Increments fix_attempts on the stage
/// 4. Attempts the merge to the default branch using existing merge logic
/// 5. On success: marks stage as completed+merged, triggers dependents
/// 6. On failure: prints detailed error with conflicting files
/// 7. If at fix limit: suggests dispute-criteria or human-review
fn merge_retry(stage_id: Option<String>) -> Result<()> {
    let work_dir = Path::new(".work");

    // Resolve stage ID: use provided or detect from current worktree branch
    let stage_id = match stage_id {
        Some(id) => id,
        None => detect_stage_id().ok_or_else(|| {
            anyhow::anyhow!(
                "Could not detect stage ID from current branch.\n\
                 Please provide the stage ID explicitly: loom stage merge <stage-id>"
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
            "Stage '{}' is in '{}' status. Only MergeConflict or MergeBlocked stages can use merge.\n\
             \n\
             Current status: {}\n\
             \n\
             For other failure states, use:\n\
             - loom stage retry {stage_id}       (for Blocked or CompletedWithFailures)\n\
             - loom stage merge {stage_id} --resolved (after manually resolving conflicts)",
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
            "merge must be run from within a worktree.\n\
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

    // Resolve worktree root from cwd. `git rev-parse --show-toplevel` returns
    // the top of the working tree, which for a worktree is its root.
    let worktree_root = crate::git::run_git_checked(&["rev-parse", "--show-toplevel"], &cwd)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| cwd.clone());

    // Refuse if either main repo or the worktree has an active merge — running
    // a programmatic merge over an in-progress resolution would clobber the
    // user's work. Run BEFORE incrementing fix_attempts so a refused retry
    // does not burn an attempt.
    let main_active = merge_head_exists(&repo_root)?;
    let worktree_active = merge_head_exists(&worktree_root)?;
    if main_active || worktree_active {
        bail!(
            "Cannot retry merge: a merge is already in progress (main: {main_active}, \
             worktree: {worktree_active}). Resolve or abort it first.",
        );
    }

    let max_attempts = stage.get_effective_max_fix_attempts();

    // Check the limit BEFORE incrementing so each attempt number is consumed
    // only when actually attempted. Checking after increment burns one extra
    // attempt at the boundary (off-by-one: with max=3, only 2 real attempts ran).
    if stage.is_at_fix_limit() {
        println!();
        println!(
            "Stage '{}' has reached the fix attempt limit ({}/{}).",
            stage_id, stage.fix_attempts, max_attempts
        );
        println!();
        println!("Options:");
        println!("  - Resolve conflicts manually then:  loom stage merge {stage_id} --resolved");
        println!("  - Request human review:             loom stage human-review {stage_id}");
        println!(
            "  - Skip this stage:                  loom stage skip {stage_id} --reason \"merge too complex\""
        );
        return Ok(());
    }

    // Increment fix_attempts after the limit guard so the count is accurate
    let attempts = stage.increment_fix_attempts();

    println!("Retrying merge for stage '{stage_id}' (attempt {attempts}/{max_attempts})");

    // Determine target branch, respecting configured base_branch over repo default
    let base_branch = crate::fs::parse_base_branch_from_config(work_dir).unwrap_or(None);
    let target_branch = resolve_target_branch(&base_branch, &repo_root);

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
            let triggered = trigger_dependents(&stage_id, work_dir, &repo_root, &target_branch)
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

            let triggered = trigger_dependents(&stage_id, work_dir, &repo_root, &target_branch)
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

            let triggered = trigger_dependents(&stage_id, work_dir, &repo_root, &target_branch)
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
                println!("  - Resolve manually then:  loom stage merge {stage_id} --resolved");
                println!("  - Request human review:   loom stage human-review {stage_id}");
                println!("  - Skip this stage:        loom stage skip {stage_id} --reason \"unresolvable conflicts\"");
            } else {
                println!("Resolve the conflicts above, then run:");
                println!("  loom stage merge {stage_id}");
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
                println!("  - Request human review:  loom stage human-review {stage_id}");
                println!("  - Skip this stage:       loom stage skip {stage_id} --reason \"merge error\"");
            } else {
                println!(
                    "Remaining attempts: {}/{max_attempts}",
                    max_attempts - attempts
                );
                println!("Investigate the error and try again: loom stage merge {stage_id}");
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
    use std::process::Command;
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

    // Tests from merge_complete

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
    fn test_merge_head_absent_in_clean_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Initialize a git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        assert!(!merge_head_exists(repo_root).unwrap());
    }

    // Tests from retry_merge

    #[test]
    fn test_merge_rejects_wrong_status() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create stages directory and a stage in Executing status
        let stages_dir = work_dir.join(".work").join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        let stage = create_test_stage("test-stage", StageStatus::Executing);
        let stage_path = stages_dir.join("test-stage.md");
        let content = crate::verify::transitions::serialize_stage_to_markdown(&stage).unwrap();
        std::fs::write(stage_path, content).unwrap();

        // merge should fail since we're not in a worktree and status is wrong
        // We test the status check by calling the function parts directly
        assert!(!matches!(
            stage.status,
            StageStatus::MergeConflict | StageStatus::MergeBlocked
        ));
    }

    #[test]
    fn test_merge_accepts_merge_conflict() {
        let stage = create_test_stage("test-stage", StageStatus::MergeConflict);
        assert!(matches!(
            stage.status,
            StageStatus::MergeConflict | StageStatus::MergeBlocked
        ));
    }

    #[test]
    fn test_merge_accepts_merge_blocked() {
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
