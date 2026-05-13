//! Knowledge stage completion logic
//!
//! Handles completion for knowledge stages which run in the main repo context
//! (no worktree) and update documentation in `doc/loom/knowledge/`.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::stage::StageType;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::acceptance_runner::{
    resolve_knowledge_acceptance_dir, run_acceptance_with_display, AcceptanceDisplayOptions,
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
    let work_dir = Path::new(".work");

    // Admin capability gate: --no-verify and --force-unsafe are
    // verification-bypass flags and require the host admin.token (held
    // outside the .work/ tree so a container-resident agent cannot
    // invoke them). Knowledge stages have no --assume-merged flag —
    // merged=true is auto-set for knowledge stages by design.
    if no_verify || force_unsafe {
        crate::commands::stage::complete::require_admin_capability(work_dir)?;
    }

    let mut stage = load_stage(stage_id, work_dir)?;

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

        // Knowledge stages auto-set merged=true since there's no branch to merge
        stage.merged = true;
        stage.status = crate::models::stage::StageStatus::Completed;
        save_stage(&stage, work_dir)?;

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
            AcceptanceDisplayOptions {
                stage_label: Some("knowledge stage"),
                show_empty_message: false,
            },
        )?)
    };

    // Handle acceptance failure - keep stage in Executing, agent can fix and retry
    if acceptance_result == Some(false) {
        eprintln!("Acceptance criteria FAILED for knowledge stage '{stage_id}'");
        eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
        anyhow::bail!("Acceptance criteria failed for knowledge stage '{stage_id}'");
    }

    // Cleanup session resources AFTER acceptance passes
    if let Some(sid) = session_id {
        cleanup_session_resources(stage_id, sid, work_dir);
    }

    // Knowledge stages auto-set merged=true since there's no branch to merge
    stage.merged = true;

    // Mark stage as completed
    stage.try_complete(None)?;
    save_stage(&stage, work_dir)?;

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
