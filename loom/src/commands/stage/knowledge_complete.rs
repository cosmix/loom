//! Knowledge stage completion logic
//!
//! Handles completion for knowledge stages which run in the main repo context
//! (no worktree) and update documentation in `doc/loom/knowledge/`.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::stage::StageType;
use crate::verify::transitions::{load_stage, trigger_dependents, update_stage};

use super::acceptance_runner::{
    print_acceptance_failure_guidance, resolve_knowledge_acceptance_dir,
    run_acceptance_with_display, AcceptanceDisplayOptions,
};
use super::session::cleanup_session_resources;

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
pub fn complete_knowledge_stage(
    stage_id: &str,
    session_id: Option<&str>,
    no_verify: bool,
    force_unsafe: bool,
) -> Result<()> {
    let work_dir_buf = crate::commands::common::work_dir_path()?;
    let work_dir: &Path = &work_dir_buf;

    let stage = load_stage(stage_id, work_dir)?;

    // Verify this is actually a knowledge stage
    debug_assert!(
        stage.stage_type == StageType::Knowledge,
        "complete_knowledge_stage called on non-knowledge stage"
    );

    // Handle --force-unsafe: bypass acceptance criteria and mark as completed directly
    if force_unsafe {
        eprintln!();
        eprintln!("⚠️  WARNING: Using --force-unsafe bypasses acceptance criteria!");
        eprintln!();

        println!(
            "Force-completing knowledge stage '{}' (was: {:?})",
            stage_id, stage.status
        );

        // Cleanup session resources if session_id provided
        if let Some(sid) = session_id {
            cleanup_session_resources(stage_id, sid, work_dir);
        }

        // This path intentionally bypasses the transition validator, but applies
        // only its owned fields to the fresh record under lock.
        update_stage(stage_id, work_dir, |current| {
            current.merged = true;
            current.status = crate::models::stage::StageStatus::Completed;
            current.completed_at = Some(chrono::Utc::now());
            current.updated_at = chrono::Utc::now();
            Ok(())
        })?;

        println!("Knowledge stage '{stage_id}' force-completed!");

        // Trigger dependent stages
        let repo_root = std::env::current_dir().context("Failed to get current directory")?;
        let target_branch = crate::fs::work_dir::load_config(work_dir)
            .ok()
            .flatten()
            .and_then(|c| c.base_branch());
        let target_branch = crate::git::branch::resolve_target_branch(&target_branch, &repo_root);
        let triggered = trigger_dependents(stage_id, work_dir, &repo_root, &target_branch)
            .context("Failed to trigger dependent stages")?;

        if !triggered.is_empty() {
            println!("Triggered {} dependent stage(s):", triggered.len());
            for dep_id in &triggered {
                println!("  → {dep_id}");
            }
        }

        return Ok(());
    }

    // Run acceptance criteria unless --no-verify
    let acceptance_result: Option<bool> = if no_verify {
        None
    } else {
        let acceptance_dir = resolve_knowledge_acceptance_dir(&stage)?;
        Some(run_acceptance_with_display(
            &stage,
            stage_id,
            acceptance_dir.as_deref(),
            work_dir,
            AcceptanceDisplayOptions {
                stage_label: Some("knowledge stage"),
                show_empty_message: false,
            },
        )?)
    };

    // Handle acceptance failure - keep stage in Executing, agent can fix and retry
    if acceptance_result == Some(false) {
        eprintln!("Acceptance criteria FAILED for knowledge stage '{stage_id}'");
        print_acceptance_failure_guidance(stage_id);
        anyhow::bail!("Acceptance criteria failed for knowledge stage '{stage_id}'");
    }

    // Cleanup session resources AFTER acceptance passes
    if let Some(sid) = session_id {
        cleanup_session_resources(stage_id, sid, work_dir);
    }

    update_stage(stage_id, work_dir, |current| {
        // Knowledge stages auto-set merged=true since there's no branch to merge.
        current.merged = true;
        current.try_complete(None)
    })?;

    println!("Knowledge stage '{stage_id}' completed!");
    println!("  (merged=true auto-set, no git merge required for knowledge stages)");

    // Trigger dependent stages
    let repo_root = std::env::current_dir().context("Failed to get current directory")?;
    let target_branch = crate::fs::work_dir::load_config(work_dir)
        .ok()
        .flatten()
        .and_then(|c| c.base_branch());
    let target_branch = crate::git::branch::resolve_target_branch(&target_branch, &repo_root);
    let triggered = trigger_dependents(stage_id, work_dir, &repo_root, &target_branch)
        .context("Failed to trigger dependent stages")?;

    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  → {dep_id}");
        }
    }

    Ok(())
}
