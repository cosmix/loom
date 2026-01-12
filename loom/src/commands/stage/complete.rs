//! Stage completion logic

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::learnings::{append_learning, Learning, LearningCategory};
use crate::fs::memory::{extract_key_notes, read_journal};
use crate::fs::task_state::read_task_state_if_exists;
use crate::git::get_branch_head;
use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};
use crate::verify::task_verification::run_task_verifications;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::session::{cleanup_session_resources, find_session_for_stage};

/// Mark a stage as complete, optionally running acceptance criteria.
/// If acceptance criteria pass, auto-verifies the stage and triggers dependents.
/// If --no-verify is used or criteria fail, marks as CompletedWithFailures for retry.
pub fn complete(stage_id: String, session_id: Option<String>, no_verify: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Resolve session_id: CLI arg > stage.session field > scan sessions directory
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));

    // Resolve worktree path from stage's worktree field
    let working_dir: Option<PathBuf> = stage
        .worktree
        .as_ref()
        .map(|w| PathBuf::from(".worktrees").join(w))
        .filter(|p| p.exists());

    // Track whether acceptance criteria passed (None = skipped via --no-verify)
    let acceptance_result: Option<bool> = if no_verify {
        // --no-verify means we skip criteria entirely (deliberate skip)
        None
    } else if !stage.acceptance.is_empty() {
        println!("Running acceptance criteria for stage '{stage_id}'...");
        if let Some(ref dir) = working_dir {
            println!("  (working directory: {})", dir.display());
        }

        let mut all_passed = true;
        for criterion in &stage.acceptance {
            println!("  → {criterion}");
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(criterion);

            if let Some(ref dir) = working_dir {
                cmd.current_dir(dir);
            }

            let status = cmd
                .status()
                .with_context(|| format!("Failed to run: {criterion}"))?;

            if !status.success() {
                all_passed = false;
                println!("  ✗ FAILED: {criterion}");
                break;
            }
            println!("  ✓ passed");
        }

        if all_passed {
            println!("All acceptance criteria passed!");
        }
        Some(all_passed)
    } else {
        // No acceptance criteria defined - treat as passed
        Some(true)
    };

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
        let repo_root = std::env::current_dir().context("Failed to get current directory")?;
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
        stage.try_complete(None)?;
        save_stage(&stage, work_dir)?;
        println!("Stage '{stage_id}' completed (skipped verification)");
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
