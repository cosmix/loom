//! Stage completion logic

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::fs::permissions::sync_worktree_permissions;
use crate::fs::session_files::find_session_for_stage;
use crate::fs::work_dir::load_config;
use crate::git::worktree::{find_repo_root_from_cwd, find_worktree_root_from_cwd};
use crate::models::stage::{StageStatus, StageType};
use crate::plan::parser::parse_plan;
use crate::plan::schema::StageDefinition;
use crate::verify::criteria::run_acceptance;
use crate::verify::goal_backward::run_goal_backward_verification;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::acceptance_runner::resolve_acceptance_dir;
use super::knowledge_complete::complete_knowledge_stage;
use super::progressive_complete::complete_with_merge;
use super::session::cleanup_session_resources;

/// Load stage definition from the active plan
fn load_stage_definition_from_plan(
    stage_id: &str,
    work_dir: &Path,
) -> Result<Option<StageDefinition>> {
    // Load config to get plan source path
    let config = match load_config(work_dir)? {
        Some(config) => config,
        None => return Ok(None),
    };

    let source_path = match config.source_path() {
        Some(path) => path,
        None => return Ok(None),
    };

    // Parse the plan
    let parsed_plan = parse_plan(&source_path)
        .with_context(|| format!("Failed to parse plan: {}", source_path.display()))?;

    // Find the stage definition by ID
    Ok(parsed_plan.stages.into_iter().find(|s| s.id == stage_id))
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
        return complete_knowledge_stage(&stage_id, session_id.as_deref(), no_verify, force_unsafe);
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
    let acceptance_dir: Option<PathBuf> =
        resolve_acceptance_dir(working_dir.as_deref(), stage.working_dir.as_deref());

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
                            let mut msg = format!(
                                "Synced permissions from worktree: {} allow, {} deny",
                                result.allow_added, result.deny_added
                            );
                            if result.worktrees_updated > 0 {
                                msg.push_str(&format!(
                                    " (propagated to {} other worktree{})",
                                    result.worktrees_updated,
                                    if result.worktrees_updated == 1 {
                                        ""
                                    } else {
                                        "s"
                                    }
                                ));
                            }
                            println!("{}", msg);
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

    // If --no-verify was used, skip verifications and merge
    if !no_verify {
        // Run goal-backward verification (truths, artifacts, wiring)
        if let Some(stage_def) = load_stage_definition_from_plan(&stage_id, work_dir)? {
            let has_goal_checks = !stage_def.truths.is_empty()
                || !stage_def.artifacts.is_empty()
                || !stage_def.wiring.is_empty();

            if has_goal_checks {
                println!("Running goal-backward verification...");
                let verification_dir = acceptance_dir.as_deref().unwrap_or(Path::new("."));
                let goal_result = run_goal_backward_verification(&stage_def, verification_dir)?;

                if !goal_result.is_passed() {
                    stage.try_complete_with_failures()?;
                    save_stage(&stage, work_dir)?;
                    println!(
                        "Stage '{stage_id}' completed with failures - goal-backward verification failed"
                    );

                    // Print gaps
                    for gap in goal_result.gaps() {
                        println!("  ✗ {:?}: {}", gap.gap_type, gap.description);
                        println!("    → {}", gap.suggestion);
                    }

                    println!();
                    println!(
                        "  Run 'loom stage retry {stage_id}' to try again after fixing issues"
                    );
                    return Ok(());
                }
                println!("Goal-backward verification passed!");
            }
        }

        // Attempt progressive merge into the merge point (base_branch)
        // Find the main repo root (not the worktree root) for merge operations.
        // When running from within a worktree, we need to merge from the main repo.
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());

        complete_with_merge(&mut stage, &repo_root, work_dir)?;
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
