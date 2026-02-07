//! Stage completion logic

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::fs::permissions::sync_worktree_permissions_with_working_dir;
use crate::fs::session_files::find_session_for_stage;
use crate::fs::work_dir::load_config;
use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::stage::{StageStatus, StageType};
use crate::plan::parser::{parse_plan, ParsedPlan};
use crate::plan::schema::{ChangeImpactConfig, ChangeImpactPolicy, StageDefinition};
use crate::verify::baseline::compare_to_baseline;
use crate::verify::goal_backward::run_goal_backward_verification;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::acceptance_runner::{
    resolve_stage_execution_paths, run_acceptance_with_display, AcceptanceDisplayOptions,
};
use super::knowledge_complete::complete_knowledge_stage;
use super::progressive_complete::complete_with_merge;
use super::session::cleanup_session_resources;

/// Load the full parsed plan from config
fn load_parsed_plan(work_dir: &Path) -> Result<Option<ParsedPlan>> {
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

    Ok(Some(parsed_plan))
}

/// Load stage definition from the active plan
fn load_stage_definition_from_plan(
    stage_id: &str,
    work_dir: &Path,
) -> Result<Option<StageDefinition>> {
    let parsed_plan = match load_parsed_plan(work_dir)? {
        Some(plan) => plan,
        None => return Ok(None),
    };

    // Find the stage definition by ID
    Ok(parsed_plan.stages.into_iter().find(|s| s.id == stage_id))
}

/// Load change impact config from the active plan
fn load_change_impact_config(work_dir: &Path) -> Result<Option<ChangeImpactConfig>> {
    let parsed_plan = match load_parsed_plan(work_dir)? {
        Some(plan) => plan,
        None => return Ok(None),
    };

    Ok(parsed_plan.metadata.loom.change_impact)
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
    if force_unsafe {
        return handle_force_unsafe_completion(stage, &stage_id, assume_merged, work_dir);
    }

    // Resolve session_id: CLI arg > stage.session field > scan sessions directory
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));

    // Resolve worktree and acceptance execution paths using shared logic
    let execution_paths = resolve_stage_execution_paths(&stage)?;
    let working_dir: Option<PathBuf> = execution_paths.worktree_root;
    let acceptance_dir: Option<PathBuf> = execution_paths.acceptance_dir;

    // Sync worktree permissions before running acceptance criteria
    sync_worktree_permissions(&working_dir, &acceptance_dir);

    // Run acceptance criteria phase
    let acceptance_result =
        run_acceptance_phase(&stage, &stage_id, no_verify, acceptance_dir.as_deref())?;

    // Handle acceptance failure - keep stage in Executing, agent can fix and retry
    // Do NOT transition state - stage stays Executing so agent can fix and re-run
    // Do NOT clean up session - agent is still alive
    if acceptance_result == Some(false) {
        eprintln!("Acceptance criteria FAILED for stage '{stage_id}'");
        eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
        anyhow::bail!("Acceptance criteria failed for stage '{stage_id}'");
    }

    // Run verification and merge phase
    run_verification_phase(
        &mut stage,
        &stage_id,
        no_verify,
        &acceptance_dir,
        session_id.as_deref(),
        work_dir,
    )?;

    Ok(())
}

/// Handle force-unsafe completion mode
///
/// Bypasses state machine validation and marks stage as completed directly.
/// This is a manual recovery command for administrative use only.
fn handle_force_unsafe_completion(
    mut stage: crate::models::stage::Stage,
    stage_id: &str,
    assume_merged: bool,
    work_dir: &Path,
) -> Result<()> {
    eprintln!();
    eprintln!("⚠️  WARNING: Using --force-unsafe bypasses state machine validation!");
    eprintln!("⚠️  This can corrupt dependency tracking and cause unexpected behavior.");
    eprintln!("⚠️  Use only for manual recovery scenarios.");
    eprintln!();

    // Best-effort permission sync before force-completing
    // Uses resolve_stage_execution_paths to get worktree paths, same as normal completion
    if let Ok(execution_paths) = resolve_stage_execution_paths(&stage) {
        sync_worktree_permissions(
            &execution_paths.worktree_root,
            &execution_paths.acceptance_dir,
        );
    }

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
        let triggered =
            trigger_dependents(stage_id, work_dir).context("Failed to trigger dependent stages")?;

        if !triggered.is_empty() {
            println!("Triggered {} dependent stage(s):", triggered.len());
            for dep_id in &triggered {
                println!("  → {dep_id}");
            }
        }
    }

    Ok(())
}

/// Sync worktree permissions with main repo
///
/// Ensures permissions are synced even if acceptance fails, allowing
/// approved permissions to persist for retry attempts.
fn sync_worktree_permissions(working_dir: &Option<PathBuf>, acceptance_dir: &Option<PathBuf>) {
    if let Some(ref dir) = working_dir {
        // Find the main repo root from the worktree path
        let repo_root = find_repo_root_from_cwd(dir);

        if let Some(ref root) = repo_root {
            match sync_worktree_permissions_with_working_dir(dir, root, acceptance_dir.as_deref()) {
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

/// Run acceptance criteria phase
///
/// Returns Some(true) if criteria passed, Some(false) if failed, None if skipped.
fn run_acceptance_phase(
    stage: &crate::models::stage::Stage,
    stage_id: &str,
    no_verify: bool,
    acceptance_dir: Option<&Path>,
) -> Result<Option<bool>> {
    // Track whether acceptance criteria passed (None = skipped via --no-verify)
    let acceptance_result: Option<bool> = if no_verify {
        // --no-verify means we skip criteria entirely (deliberate skip)
        None
    } else {
        Some(run_acceptance_with_display(
            stage,
            stage_id,
            acceptance_dir,
            AcceptanceDisplayOptions {
                stage_label: Some("stage"),
                show_empty_message: false,
            },
        )?)
    };

    Ok(acceptance_result)
}

/// Run verification phase (goal-backward verification and change impact comparison)
///
/// If verifications pass, performs progressive merge. If --no-verify is used, skips all checks.
fn run_verification_phase(
    stage: &mut crate::models::stage::Stage,
    stage_id: &str,
    no_verify: bool,
    acceptance_dir: &Option<PathBuf>,
    session_id: Option<&str>,
    work_dir: &Path,
) -> Result<()> {
    if !no_verify {
        // Run goal-backward verification (truths, artifacts, wiring)
        if let Some(stage_def) = load_stage_definition_from_plan(stage_id, work_dir)? {
            if stage_def.has_any_goal_checks() {
                println!("Running goal-backward verification...");
                let verification_dir = acceptance_dir.as_deref().unwrap_or(Path::new("."));
                let goal_result = run_goal_backward_verification(&stage_def, verification_dir)?;

                if !goal_result.is_passed() {
                    // Print gaps
                    for gap in goal_result.gaps() {
                        eprintln!("  ✗ {:?}: {}", gap.gap_type, gap.description);
                        eprintln!("    → {}", gap.suggestion);
                    }

                    eprintln!();
                    eprintln!("Goal-backward verification FAILED for stage '{stage_id}'");
                    eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
                    anyhow::bail!("Goal-backward verification failed for stage '{stage_id}'");
                }
                println!("Goal-backward verification passed!");
            }
        }

        // Run change impact comparison if configured
        if let Some(change_impact_config) = load_change_impact_config(work_dir)? {
            if change_impact_config.policy != ChangeImpactPolicy::Skip {
                println!("Running change impact comparison...");
                let comparison_dir = acceptance_dir.as_deref();

                match compare_to_baseline(stage_id, &change_impact_config, comparison_dir, work_dir)
                {
                    Ok(impact) => {
                        if !impact.comparison_succeeded {
                            eprintln!(
                                "Warning: Change impact comparison failed to run, continuing anyway"
                            );
                        } else {
                            // Print summary
                            println!("  {}", impact.summary());

                            // Print details if there are new failures
                            if impact.has_new_failures() {
                                println!("  New failures detected:");
                                for failure in &impact.new_failures {
                                    println!("    - {}", failure);
                                }
                            }

                            if !impact.fixed_failures.is_empty() {
                                println!("  Fixed failures:");
                                for fixed in &impact.fixed_failures {
                                    println!("    + {}", fixed);
                                }
                            }

                            // Check policy and fail if necessary
                            if impact.has_new_failures()
                                && change_impact_config.policy == ChangeImpactPolicy::Fail
                            {
                                eprintln!("Change impact check FAILED for stage '{stage_id}' - new failures introduced");
                                eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
                                anyhow::bail!("Change impact check failed for stage '{stage_id}' - new failures introduced");
                            }

                            if impact.has_new_failures()
                                && change_impact_config.policy == ChangeImpactPolicy::Warn
                            {
                                eprintln!("⚠️  Warning: New failures introduced, but continuing due to 'warn' policy");
                            }
                        }
                    }
                    Err(e) => {
                        // No baseline exists or comparison failed - just warn and continue
                        eprintln!("Warning: Change impact comparison skipped: {e}");
                    }
                }
            }
        }

        // All verifications passed - NOW clean up session resources
        if let Some(sid) = session_id {
            cleanup_session_resources(stage_id, sid, work_dir);
        }

        // Attempt progressive merge into the merge point (base_branch)
        // Find the main repo root (not the worktree root) for merge operations.
        // When running from within a worktree, we need to merge from the main repo.
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());

        complete_with_merge(stage, &repo_root, work_dir)?;
    } else {
        // --no-verify: Skip verifications and merge, just mark as completed
        // Merged flag semantics for this path:
        // - merged=false (merge was skipped entirely)
        // - Dependents will NOT be triggered automatically
        // - User must manually merge and use --force-unsafe --assume-merged to trigger dependents
        stage.try_complete(None)?;
        save_stage(stage, work_dir)?;
        println!("Stage '{stage_id}' completed (skipped verification)");
        println!("⚠️  Note: Merge was skipped. Stage NOT marked as merged.");
        println!("⚠️  Dependent stages will NOT be automatically triggered.");
    }

    Ok(())
}
