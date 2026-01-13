//! Stage completion logic

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::fs::learnings::{append_learning, Learning, LearningCategory};
use crate::fs::memory::{extract_key_notes, read_journal};
use crate::fs::permissions::sync_worktree_permissions;
use crate::fs::task_state::read_task_state_if_exists;
use crate::git::get_branch_head;
use crate::git::worktree::{find_repo_root_from_cwd, find_worktree_root_from_cwd};
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};
use crate::verify::criteria::run_acceptance;
use crate::verify::task_verification::run_task_verifications;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::session::{cleanup_session_resources, find_session_for_stage};

/// Complete a knowledge stage without requiring merge.
///
/// Knowledge stages run in the main repo context (no worktree) and update
/// documentation in `doc/loom/knowledge/`. Since they don't have a branch
/// to merge, we skip merge entirely and auto-set `merged=true`.
///
/// # Process
/// 1. Run acceptance criteria if specified (in main repo context)
/// 2. Skip merge attempt entirely (no branch to merge)
/// 3. Auto-set merged=true (no actual merge needed)
/// 4. Mark stage as Completed
/// 5. Trigger dependent stages
fn complete_knowledge_stage(
    stage_id: &str,
    session_id: Option<&str>,
    no_verify: bool,
) -> Result<()> {
    let work_dir = Path::new(".work");
    let mut stage = load_stage(stage_id, work_dir)?;

    // Run acceptance criteria unless --no-verify
    let acceptance_result: Option<bool> = if no_verify {
        None
    } else if !stage.acceptance.is_empty() {
        println!("Running acceptance criteria for knowledge stage '{stage_id}'...");

        // Knowledge stages run in main repo context, not a worktree
        // Use stage.working_dir if set, otherwise current directory
        let acceptance_dir: Option<PathBuf> = stage
            .working_dir
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.exists());

        if let Some(ref dir) = acceptance_dir {
            println!("  (working directory: {})", dir.display());
        }

        let result = run_acceptance(&stage, acceptance_dir.as_deref())
            .context("Failed to run acceptance criteria")?;

        for criterion_result in result.results() {
            if criterion_result.success {
                println!("  ✓ passed: {}", criterion_result.command);
            } else if criterion_result.timed_out {
                println!("  ✗ TIMEOUT: {}", criterion_result.command);
            } else {
                println!("  ✗ FAILED: {}", criterion_result.command);
            }
        }

        if result.all_passed() {
            println!("All acceptance criteria passed!");
        }
        Some(result.all_passed())
    } else {
        // No acceptance criteria defined - treat as passed
        Some(true)
    };

    // Cleanup session resources if session_id provided
    if let Some(sid) = session_id {
        cleanup_session_resources(stage_id, sid, work_dir);
    }

    // Handle acceptance failure
    if acceptance_result == Some(false) {
        stage.try_complete_with_failures()?;
        save_stage(&stage, work_dir)?;
        println!("Knowledge stage '{stage_id}' completed with failures - acceptance criteria did not pass");
        println!("  Run 'loom stage retry {stage_id}' to try again after fixing issues");
        return Ok(());
    }

    // Knowledge stages auto-set merged=true since there's no branch to merge
    stage.merged = true;

    // Mark stage as completed
    stage.try_complete(None)?;
    save_stage(&stage, work_dir)?;

    println!("Knowledge stage '{stage_id}' completed!");
    println!("  (merged=true auto-set, no git merge required for knowledge stages)");

    // Trigger dependent stages
    let triggered = trigger_dependents(stage_id, work_dir)
        .context("Failed to trigger dependent stages")?;

    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  → {dep_id}");
        }
    }

    Ok(())
}

/// Mark a stage as complete, optionally running acceptance criteria.
/// If acceptance criteria pass, auto-verifies the stage and triggers dependents.
/// If --no-verify is used or criteria fail, marks as CompletedWithFailures for retry.
/// If --force-unsafe is used, bypasses state machine and marks stage as Completed from any state.
pub fn complete(
    stage_id: String,
    session_id: Option<String>,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
) -> Result<()> {
    let work_dir = Path::new(".work");

    let stage = load_stage(&stage_id, work_dir)?;

    // Route knowledge stages to specialized completion (no merge required)
    if stage.stage_type == StageType::Knowledge {
        return complete_knowledge_stage(&stage_id, session_id.as_deref(), no_verify);
    }

    // From here on, we need mutable stage for standard completion
    let mut stage = stage;

    // Handle --force-unsafe: bypass state machine and mark as completed directly
    // Merged flag semantics for this path:
    // - merged=true ONLY if --assume-merged is provided (manual merge assumed)
    // - merged=false otherwise (manual merge needed, dependents won't trigger)
    if force_unsafe {
        eprintln!();
        eprintln!("⚠️  WARNING: Using --force-unsafe bypasses state machine validation!");
        eprintln!("⚠️  This can corrupt dependency tracking and cause unexpected behavior.");
        eprintln!("⚠️  Use only for manual recovery scenarios.");
        eprintln!();

        println!(
            "Force-completing stage '{}' (was: {:?})",
            stage_id, stage.status
        );

        // INTENTIONAL STATE MACHINE BYPASS: This is a manual recovery command
        // that allows administrators to force completion from any state.
        // This is the ONLY place where direct status assignment is acceptable.
        stage.status = StageStatus::Completed;

        // Only set merged=true if explicitly requested via --assume-merged
        if assume_merged {
            stage.merged = true;
            println!("  → Stage marked as merged (manual merge assumed)");
        } else {
            stage.merged = false;
            eprintln!();
            eprintln!("⚠️  WARNING: Stage NOT marked as merged (--assume-merged not provided).");
            eprintln!("⚠️  Dependent stages will NOT be automatically triggered.");
            eprintln!("⚠️  If you manually merged the branch, re-run with --assume-merged to trigger dependents.");
            eprintln!();
        }

        save_stage(&stage, work_dir)?;
        println!("Stage '{stage_id}' force-completed!");

        // Only trigger dependent stages if merged=true (i.e., --assume-merged was used)
        if stage.merged {
            let triggered = trigger_dependents(&stage_id, work_dir)
                .context("Failed to trigger dependent stages")?;

            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  → {dep_id}");
                }
            }
        }
        return Ok(());
    }

    // Resolve session_id: CLI arg > stage.session field > scan sessions directory
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));

    // Resolve worktree path: first try detecting from cwd, then fall back to stage.worktree field
    let cwd = std::env::current_dir().ok();
    let working_dir: Option<PathBuf> = cwd
        .as_ref()
        .and_then(|p| find_worktree_root_from_cwd(p))
        .or_else(|| {
            stage
                .worktree
                .as_ref()
                .map(|w| PathBuf::from(".worktrees").join(w))
                .filter(|p| p.exists())
        });

    // Resolve acceptance criteria working directory:
    // If stage has a working_dir set, join it with the worktree root
    // Special case: "." means use the worktree root directly
    let acceptance_dir: Option<PathBuf> = resolve_acceptance_dir(
        working_dir.as_deref(),
        stage.working_dir.as_deref(),
    );

    // Debug logging for path resolution
    if let Some(ref worktree_root) = working_dir {
        eprintln!(
            "Debug: Path resolution - worktree_root={}, working_dir={:?}",
            worktree_root.display(),
            stage.working_dir
        );
        if let Some(ref resolved) = acceptance_dir {
            eprintln!("Debug: Resolved acceptance_dir: {}", resolved.display());
        }
    } else {
        eprintln!("Debug: No worktree root found, acceptance_dir will be None");
    }

    // Track whether acceptance criteria passed (None = skipped via --no-verify)
    let acceptance_result: Option<bool> = if no_verify {
        // --no-verify means we skip criteria entirely (deliberate skip)
        None
    } else if !stage.acceptance.is_empty() {
        println!("Running acceptance criteria for stage '{stage_id}'...");
        if let Some(ref dir) = acceptance_dir {
            println!("  (working directory: {})", dir.display());
        }

        let result = run_acceptance(&stage, acceptance_dir.as_deref())
            .context("Failed to run acceptance criteria")?;

        // Print results for each criterion
        for criterion_result in result.results() {
            if criterion_result.success {
                println!("  ✓ passed: {}", criterion_result.command);
            } else if criterion_result.timed_out {
                println!("  ✗ TIMEOUT: {}", criterion_result.command);
            } else {
                println!("  ✗ FAILED: {}", criterion_result.command);
            }
        }

        if result.all_passed() {
            println!("All acceptance criteria passed!");
        }
        Some(result.all_passed())
    } else {
        // No acceptance criteria defined - treat as passed
        Some(true)
    };

    // Sync worktree permissions to main repo (non-fatal - warn on error)
    if acceptance_result != Some(false) {
        if let Some(ref dir) = working_dir {
            // Find the main repo root from the worktree path
            let repo_root = find_repo_root_from_cwd(dir);

            if let Some(ref root) = repo_root {
                match sync_worktree_permissions(dir, root) {
                    Ok(result) => {
                        if result.allow_added > 0 || result.deny_added > 0 {
                            println!(
                                "Synced permissions from worktree: {} allow, {} deny",
                                result.allow_added, result.deny_added
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to sync worktree permissions: {e}");
                    }
                }
            }
        }
    }

    // Cleanup terminal resources based on backend type
    cleanup_terminal_for_stage(&stage_id, session_id.as_deref(), work_dir);

    // Cleanup session resources (update session status, remove signal)
    if let Some(ref sid) = session_id {
        cleanup_session_resources(&stage_id, sid, work_dir);
    }

    // Handle acceptance failure - mark as CompletedWithFailures and exit early
    // (but not for --no-verify which is a deliberate skip)
    if acceptance_result == Some(false) {
        stage.try_complete_with_failures()?;
        save_stage(&stage, work_dir)?;
        println!("Stage '{stage_id}' completed with failures - acceptance criteria did not pass");
        println!("  Run 'loom stage retry {stage_id}' to try again after fixing issues");
        return Ok(());
    }

    // If --no-verify was used, skip task verifications and merge
    if !no_verify {
        // Run task verifications if task state exists
        if let Some(task_state) = read_task_state_if_exists(work_dir, &stage_id)? {
            println!("Running task verifications...");
            let worktree_path = working_dir.as_deref().unwrap_or(Path::new("."));

            // Collect all verification rules from tasks
            let all_rules: Vec<_> = task_state
                .tasks
                .iter()
                .flat_map(|t| t.verification.iter().cloned())
                .collect();

            if !all_rules.is_empty() {
                // Build outputs map from stage outputs (convert Value to String)
                let outputs: HashMap<String, String> = stage
                    .outputs
                    .iter()
                    .map(|o| {
                        let value_str = match &o.value {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        (o.key.clone(), value_str)
                    })
                    .collect();

                let task_results = run_task_verifications(&all_rules, worktree_path, &outputs);
                let all_tasks_passed = task_results.iter().all(|r| r.passed);

                if !all_tasks_passed {
                    stage.try_complete_with_failures()?;
                    save_stage(&stage, work_dir)?;
                    println!("Stage '{stage_id}' completed with failures - task verifications did not pass");
                    for result in &task_results {
                        if !result.passed {
                            println!("  Task verification FAILED: {}", result.message);
                        }
                    }
                    return Ok(());
                }
                println!("All task verifications passed!");
            }
        }

        // Attempt progressive merge into the merge point (base_branch)
        // Merged flag semantics for this path (normal completion):
        // - merged=true ONLY if git merge succeeds (or fast-forward/already-merged)
        // - merged=false if merge has conflicts or errors
        // - merged=true even if NoBranch (branch was already cleaned up)
        //
        // Find the main repo root (not the worktree root) for merge operations.
        // When running from within a worktree, we need to merge from the main repo.
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let repo_root = find_repo_root_from_cwd(&cwd)
            .unwrap_or_else(|| cwd.clone());
        let merge_point = get_merge_point(work_dir)?;

        // Capture the completed commit SHA before merge (the HEAD of the stage branch)
        let branch_name = format!("loom/{stage_id}");
        let completed_commit = get_branch_head(&branch_name, &repo_root).ok();

        println!("Attempting progressive merge into '{merge_point}'...");
        match merge_completed_stage(&stage, &repo_root, &merge_point) {
            Ok(ProgressiveMergeResult::Success { files_changed }) => {
                println!("  ✓ Merged {files_changed} file(s) into '{merge_point}'");
                stage.completed_commit = completed_commit;
                stage.merged = true;
            }
            Ok(ProgressiveMergeResult::FastForward) => {
                println!("  ✓ Fast-forward merge into '{merge_point}'");
                stage.completed_commit = completed_commit;
                stage.merged = true;
            }
            Ok(ProgressiveMergeResult::AlreadyMerged) => {
                println!("  ✓ Already up to date with '{merge_point}'");
                stage.completed_commit = completed_commit;
                stage.merged = true;
            }
            Ok(ProgressiveMergeResult::NoBranch) => {
                println!("  → No branch to merge (already cleaned up)");
                stage.merged = true;
            }
            Ok(ProgressiveMergeResult::Conflict { conflicting_files }) => {
                println!("  ✗ Merge conflict detected!");
                println!("    Conflicting files:");
                for file in &conflicting_files {
                    println!("      - {file}");
                }
                println!();
                println!("    Stage transitioning to MergeConflict status.");
                println!("    Resolve conflicts and run: loom stage merge-complete {stage_id}");
                stage.try_mark_merge_conflict()?;
                save_stage(&stage, work_dir)?;
                // Don't trigger dependents when there's a conflict
                return Ok(());
            }
            Err(e) => {
                eprintln!("Progressive merge failed: {e}");
                stage.try_mark_merge_blocked()?;
                save_stage(&stage, work_dir)?;
                eprintln!("Stage '{stage_id}' marked as MergeBlocked");
                eprintln!("  Fix the issue and run: loom stage retry {stage_id}");
                return Ok(());
            }
        }

        // Mark stage as completed - only after all checks pass
        stage.try_complete(None)?;
        save_stage(&stage, work_dir)?;

        // Promote key decisions from memory to learnings
        if let Some(ref sid) = session_id {
            if let Ok(journal) = read_journal(work_dir, sid) {
                let decisions = extract_key_notes(&journal);
                for decision in decisions {
                    let learning = Learning {
                        timestamp: chrono::Utc::now(),
                        stage_id: stage_id.clone(),
                        description: decision,
                        correction: None,
                        source: None,
                    };
                    let _ = append_learning(work_dir, LearningCategory::Pattern, &learning);
                }
            }
        }

        println!("Stage '{stage_id}' completed!");

        // Trigger dependent stages
        let triggered = trigger_dependents(&stage_id, work_dir)
            .context("Failed to trigger dependent stages")?;

        if !triggered.is_empty() {
            println!("Triggered {} dependent stage(s):", triggered.len());
            for dep_id in &triggered {
                println!("  → {dep_id}");
            }
        }
    } else {
        // --no-verify: Skip verifications and merge, just mark as completed
        // Merged flag semantics for this path:
        // - merged=false (merge was skipped entirely)
        // - Dependents will NOT be triggered automatically
        // - User must manually merge and use --force-unsafe --assume-merged to trigger dependents
        stage.try_complete(None)?;
        save_stage(&stage, work_dir)?;
        println!("Stage '{stage_id}' completed (skipped verification)");
        println!("⚠️  Note: Merge was skipped. Stage NOT marked as merged.");
        println!("⚠️  Dependent stages will NOT be automatically triggered.");
    }

    Ok(())
}

/// Clean up terminal resources for a stage based on backend type
///
/// For native backend, process cleanup is handled by the orchestrator via PID.
/// No additional cleanup needed here.
pub(super) fn cleanup_terminal_for_stage(
    _stage_id: &str,
    _session_id: Option<&str>,
    _work_dir: &Path,
) {
    // Native backend cleanup is handled by orchestrator via PID
    // No additional cleanup needed here
}

/// Helper function to resolve acceptance directory from worktree root and working_dir.
/// Exposed for testing.
///
/// # Arguments
/// * `worktree_root` - The root of the worktree (e.g., ".worktrees/stage-id")
/// * `working_dir` - The stage's working_dir setting (e.g., ".", "loom", None)
///
/// # Returns
/// The resolved path for running acceptance criteria
pub fn resolve_acceptance_dir(
    worktree_root: Option<&Path>,
    working_dir: Option<&str>,
) -> Option<PathBuf> {
    match (worktree_root, working_dir) {
        (Some(root), Some(subdir)) => {
            // Handle "." special case - use worktree root directly
            if subdir == "." {
                Some(root.to_path_buf())
            } else {
                let full_path = root.join(subdir);
                if full_path.exists() {
                    Some(full_path)
                } else {
                    // Fall back to worktree root if subdirectory doesn't exist
                    eprintln!(
                        "Warning: stage working_dir '{subdir}' does not exist in worktree at '{}', using worktree root",
                        full_path.display()
                    );
                    Some(root.to_path_buf())
                }
            }
        }
        (Some(root), None) => {
            // No working_dir specified, use worktree root
            Some(root.to_path_buf())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_acceptance_dir_dot_uses_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("."));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_subdir_uses_worktree_root_joined() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Create the subdirectory
        let subdir_path = worktree_root.join("loom");
        std::fs::create_dir_all(&subdir_path).unwrap();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("loom"));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), subdir_path);
    }

    #[test]
    fn test_resolve_acceptance_dir_missing_subdir_falls_back_to_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Don't create the subdirectory - it should fall back to root
        let result = resolve_acceptance_dir(Some(worktree_root), Some("nonexistent"));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_none_working_dir_uses_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        let result = resolve_acceptance_dir(Some(worktree_root), None);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_no_worktree_returns_none() {
        let result = resolve_acceptance_dir(None, Some("."));

        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_acceptance_dir_nested_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Create a nested subdirectory
        let subdir_path = worktree_root.join("packages/core");
        std::fs::create_dir_all(&subdir_path).unwrap();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("packages/core"));

        assert!(result.is_some());
        assert_eq!(result.unwrap(), subdir_path);
    }
}
