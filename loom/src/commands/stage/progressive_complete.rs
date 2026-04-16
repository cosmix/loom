//! Progressive merge integration for stage completion
//!
//! This module handles the git merge operations that occur when a stage
//! completes successfully with passing acceptance criteria.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::{cleanup_after_merge, CleanupConfig};
use crate::git::get_branch_head;
use crate::models::stage::Stage;
use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};
use crate::verify::transitions::save_stage;

/// Result of attempting to merge a completed stage
pub enum MergeOutcome {
    /// Merge succeeded - stage can be marked completed
    Success,
    /// Merge conflict - stage should be marked MergeConflict
    Conflict,
    /// Merge blocked - stage should be marked MergeBlocked
    Blocked,
}

/// Attempt to progressively merge a completed stage into the merge point.
///
/// This function handles the git merge operations and updates the stage's
/// merge-related fields (merged, completed_commit).
///
/// # Returns
/// - `MergeOutcome::Success` if merge succeeded (stage should be marked Completed)
/// - `MergeOutcome::Conflict` if merge conflict (stage already marked MergeConflict)
/// - `MergeOutcome::Blocked` if merge failed (stage already marked MergeBlocked)
pub fn attempt_progressive_merge(
    stage: &mut Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<MergeOutcome> {
    let merge_point = get_merge_point(work_dir)?;

    // Capture the completed commit SHA before merge (the HEAD of the stage branch).
    // This represents the stage's work output and is set for ALL outcomes (including
    // conflict) so the orchestrator can later verify merge resolution via git ancestry.
    let branch_name = branch_name_for_stage(&stage.id);
    let completed_commit = get_branch_head(&branch_name, repo_root).ok();
    stage.completed_commit = completed_commit;

    println!("Attempting progressive merge into '{merge_point}'...");
    match merge_completed_stage(stage, repo_root, &merge_point) {
        Ok(ProgressiveMergeResult::Success { files_changed }) => {
            println!("  ✓ Merged {files_changed} file(s) into '{merge_point}'");
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::FastForward) => {
            println!("  ✓ Fast-forward merge into '{merge_point}'");
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::AlreadyMerged) => {
            println!("  ✓ Already up to date with '{merge_point}'");
            stage.merged = true;
            Ok(MergeOutcome::Success)
        }
        Ok(ProgressiveMergeResult::NoBranch) => {
            tracing::error!(
                stage_id = %stage.id,
                "Progressive merge: branch missing — cannot verify merge succeeded"
            );
            Ok(MergeOutcome::Blocked)
        }
        Ok(ProgressiveMergeResult::Conflict { conflicting_files }) => {
            println!("  ✗ Merge conflict detected!");
            println!("    Conflicting files:");
            for file in &conflicting_files {
                println!("      - {file}");
            }
            println!();
            println!("    Stage transitioning to MergeConflict status.");
            stage.try_mark_merge_conflict()?;
            save_stage(stage, work_dir)?;

            // Try to auto-spawn a merge resolver session
            use super::merge_resolver::MergeResolverResult;
            match super::merge_resolver::spawn_merge_resolver(
                stage,
                &conflicting_files,
                &merge_point,
                repo_root,
                work_dir,
            ) {
                Ok(MergeResolverResult::DaemonManaged) => {
                    println!(
                        "    Daemon is running - merge resolution will be handled automatically."
                    );
                }
                Ok(MergeResolverResult::Spawned(id)) => {
                    println!("    Spawned merge resolver session: {id}");
                }
                Err(e) => {
                    eprintln!("    Failed to spawn merge resolver: {e}");
                    println!(
                        "    Resolve conflicts manually and run: loom stage merge {} --resolved",
                        stage.id
                    );
                }
            }

            Ok(MergeOutcome::Conflict)
        }
        Err(e) => {
            eprintln!("Progressive merge failed: {e}");
            stage.try_mark_merge_blocked()?;
            save_stage(stage, work_dir)?;
            eprintln!("Stage '{}' marked as MergeBlocked", stage.id);
            eprintln!("  Fix the issue and run: loom stage retry {}", stage.id);
            Ok(MergeOutcome::Blocked)
        }
    }
}

/// Complete a stage with merge, triggering dependents on success.
///
/// This is the standard completion path for stages after acceptance criteria pass.
/// It attempts progressive merge and marks the stage as completed.
pub fn complete_with_merge(stage: &mut Stage, repo_root: &Path, work_dir: &Path) -> Result<bool> {
    match attempt_progressive_merge(stage, repo_root, work_dir)? {
        MergeOutcome::Success => {
            // Mark stage as completed - only after merge succeeds
            stage.try_complete(None)?;
            save_stage(stage, work_dir)?;

            println!("Stage '{}' completed!", stage.id);

            // Trigger dependent stages
            let target_branch = crate::fs::work_dir::load_config(work_dir)
                .ok()
                .flatten()
                .and_then(|c| c.base_branch());
            let target_branch =
                crate::git::branch::resolve_target_branch(&target_branch, repo_root);
            let triggered = crate::verify::transitions::trigger_dependents(
                &stage.id,
                work_dir,
                repo_root,
                &target_branch,
            )
            .context("Failed to trigger dependent stages")?;

            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  → {dep_id}");
                }
            }

            // Clean up worktree and branch after successful merge
            let cleanup_config = CleanupConfig {
                verbose: true,
                force_worktree_removal: false,
                force_branch_deletion: false,
                prune_worktrees: true,
            };

            match cleanup_after_merge(&stage.id, repo_root, &cleanup_config) {
                Ok(result) => {
                    if result.worktree_removed {
                        println!("  Removed worktree: .worktrees/{}", stage.id);
                    }
                    if result.branch_deleted {
                        println!("  Deleted branch: {}", branch_name_for_stage(&stage.id));
                    }
                    if !result.warnings.is_empty() {
                        for warning in &result.warnings {
                            eprintln!("  Warning: {warning}");
                        }
                    }
                }
                Err(e) => {
                    // Cleanup failure is not fatal - stage is already completed
                    eprintln!("  Warning: Failed to clean up stage resources: {e}");
                    eprintln!(
                        "  You can manually clean up with: loom worktree remove {}",
                        stage.id
                    );
                }
            }

            Ok(true)
        }
        MergeOutcome::Conflict => {
            bail!(
                "Merge conflict detected for stage '{}'.\n\
                 A resolution session has been spawned to handle the merge.\n\
                 Your work is committed on the stage branch -- this session should exit now.\n\
                 Do NOT attempt to resolve the merge conflict yourself.",
                stage.id
            );
        }
        MergeOutcome::Blocked => {
            bail!(
                "Merge blocked for stage '{}'.\n\
                 The stage has been marked MergeBlocked.\n\
                 This session should exit now. Fix the issue and run: loom stage retry {}",
                stage.id,
                stage.id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for `attempt_progressive_merge` (PLAN-fix-phantom-merge.md).
    //!
    //! Historically, the `NoBranch` arm of the inner match wrote `merged = true`
    //! under the assumption that "branch already cleaned up" implied "already
    //! merged." That assumption is wrong: if the branch is missing before any
    //! merge attempt happened, we cannot verify anything landed. Fix 7 replaces
    //! the arm with `MergeOutcome::Blocked` and does NOT write `merged = true`.
    //!
    //! Setting up a real merge that returns `NoBranch` naturally is tricky —
    //! the function's precondition calls `get_branch_head` which errors if the
    //! branch doesn't exist. The tests below build a minimal real-git repo
    //! without the expected `loom/<stage-id>` branch, which is the same
    //! observable condition (`NoBranch`) from the caller's perspective: the
    //! function must return `Blocked` (or surface an error) and MUST NOT leave
    //! the stage with `merged = true`.
    //!
    //! End-to-end phantom-merge prevention across recovery and daemon paths is
    //! additionally exercised by the integration suite in `tests/phantom_merge.rs`.
    use std::process::Command;

    use tempfile::TempDir;

    use super::{attempt_progressive_merge, MergeOutcome};
    use crate::models::stage::{Stage, StageStatus};

    /// Build a real git repo with a `.work` directory and a `config.toml` that
    /// points at `main` as the base branch. Returns the repo root TempDir.
    fn init_repo_with_work_dir() -> TempDir {
        let temp_dir = TempDir::new().expect("tempdir");
        let repo_root = temp_dir.path();

        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(repo_root)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(repo_root)
            .output()
            .expect("git config email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(repo_root)
            .output()
            .expect("git config name");
        std::fs::write(repo_root.join("README.md"), "r").expect("write README");
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(repo_root)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(repo_root)
            .output()
            .expect("git commit");
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(repo_root)
            .output()
            .expect("rename to main");

        // Create a minimal .work directory with config.toml so `get_merge_point`
        // can resolve to "main".
        let work_dir = repo_root.join(".work");
        std::fs::create_dir_all(&work_dir).expect("mkdir .work");
        std::fs::write(work_dir.join("config.toml"), "base_branch = \"main\"\n")
            .expect("write config.toml");

        temp_dir
    }

    fn make_stage(id: &str) -> Stage {
        let mut stage = Stage::new(id.to_string(), Some(format!("test {id}")));
        stage.id = id.to_string();
        stage.status = StageStatus::Executing;
        stage
    }

    /// Fix 7: `attempt_progressive_merge` must NOT set `merged = true` when
    /// the stage branch is missing. The old NoBranch arm silently wrote
    /// `merged = true` — a phantom merge.
    ///
    /// Without a `loom/<stage-id>` branch, `merge_completed_stage` returns
    /// `NoBranch`, which the new code translates to `MergeOutcome::Blocked`.
    /// Some git-layer paths may surface the missing branch as an error instead;
    /// either way, the invariant we care about is the same: the stage's
    /// `merged` flag must remain false.
    #[test]
    fn no_branch_does_not_mark_merged() {
        let repo = init_repo_with_work_dir();
        let repo_root = repo.path();
        let work_dir = repo_root.join(".work");

        let mut stage = make_stage("stage-no-branch");
        assert!(!stage.merged, "precondition: stage starts unmerged");

        // No loom/stage-no-branch branch exists. The progressive merge should
        // refuse to mark the stage merged regardless of how the missing branch
        // surfaces (Blocked outcome, or an Err from the deeper git call).
        let outcome = attempt_progressive_merge(&mut stage, repo_root, &work_dir);

        match outcome {
            Ok(MergeOutcome::Blocked) => {
                // Fix 7's intended behavior.
            }
            Ok(MergeOutcome::Success) => {
                panic!(
                    "phantom merge: NoBranch should not produce Success. stage.merged = {}",
                    stage.merged
                );
            }
            Ok(MergeOutcome::Conflict) => {
                panic!("unexpected Conflict from missing branch");
            }
            Err(_) => {
                // Some implementations may surface missing branch as an error
                // (e.g., if `get_branch_head` is called before the NoBranch
                // check in a future refactor). Either way the assertion below
                // is what matters.
            }
        }

        assert!(
            !stage.merged,
            "regression: missing stage branch must NOT set merged=true (phantom merge prevention)"
        );
    }
}
