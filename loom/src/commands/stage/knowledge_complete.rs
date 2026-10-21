//! Knowledge stage completion logic
//!
//! Handles completion for knowledge stages which run in the main repo context
//! (no worktree) and update documentation in `doc/loom/knowledge/`.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::stage::{Stage, StageType};
use crate::verify::transitions::{load_stage, trigger_dependents, update_stage};

use super::acceptance_runner::{
    print_acceptance_failure_guidance, resolve_knowledge_execution_paths,
    run_acceptance_with_display, AcceptanceDisplayOptions,
};
use super::session::cleanup_session_resources;

/// Complete a knowledge stage without requiring merge, from the operator's
/// shell. A knowledge session's completion is a host job instead: the daemon
/// runs its acceptance in the repository root and applies the transition
/// (`host_verify::apply`).
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

    if force_unsafe {
        return force_complete_knowledge(&stage, session_id, work_dir);
    }
    let snapshot = if no_verify {
        None
    } else {
        Some(crate::handoff::completion::content::VerificationSnapshot::capture(&stage, work_dir)?)
    };
    if !no_verify {
        verify_knowledge_acceptance(&stage, work_dir)?;
    }

    finish_knowledge_stage(&stage, session_id, snapshot.as_ref(), work_dir)
}

pub(super) fn finish_knowledge_stage(
    stage: &Stage,
    session_id: Option<&str>,
    snapshot: Option<&crate::handoff::completion::content::VerificationSnapshot>,
    work_dir: &Path,
) -> Result<()> {
    let stage_id = &stage.id;
    if let Some(snapshot) = &snapshot {
        snapshot.validate(stage, work_dir)?;
    }
    // Cleanup session resources AFTER acceptance passes
    if let Some(sid) = session_id {
        cleanup_session_resources(stage_id, sid, work_dir);
    }

    update_stage(stage_id, work_dir, |current| {
        if let Some(snapshot) = &snapshot {
            snapshot.validate_locked(current, work_dir)?;
        }
        // Knowledge stages auto-set merged=true since there's no branch to merge.
        current.merged = true;
        current.completed_commit = snapshot.map(|snapshot| snapshot.commit.clone());
        // An operator's force-complete lands a stage still awaiting review.
        current.review_reason = None;
        current.try_complete(None)
    })?;

    println!("Knowledge stage '{stage_id}' completed!");
    println!("  (merged=true auto-set, no git merge required for knowledge stages)");

    trigger_knowledge_dependents(stage_id, work_dir)
}

fn force_complete_knowledge(
    stage: &Stage,
    session_id: Option<&str>,
    work_dir: &Path,
) -> Result<()> {
    let stage_id = &stage.id;
    eprintln!();
    eprintln!("⚠️  WARNING: Using --force-unsafe bypasses acceptance criteria!");
    eprintln!();
    println!(
        "Force-completing knowledge stage '{}' (was: {:?})",
        stage_id, stage.status
    );
    if let Some(sid) = session_id {
        cleanup_session_resources(stage_id, sid, work_dir);
    }
    update_stage(stage_id, work_dir, |current| {
        current.merged = true;
        current.status = crate::models::stage::StageStatus::Completed;
        current.completed_at = Some(chrono::Utc::now());
        current.updated_at = chrono::Utc::now();
        Ok(())
    })?;
    println!("Knowledge stage '{stage_id}' force-completed!");
    trigger_knowledge_dependents(stage_id, work_dir)
}

fn trigger_knowledge_dependents(stage_id: &str, work_dir: &Path) -> Result<()> {
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

fn verify_knowledge_acceptance(stage: &Stage, work_dir: &Path) -> Result<()> {
    let paths = resolve_knowledge_execution_paths(stage)?;
    let passed = run_acceptance_with_display(
        stage,
        &stage.id,
        &paths,
        work_dir,
        AcceptanceDisplayOptions {
            stage_label: Some("knowledge stage"),
            show_empty_message: false,
        },
    )?;
    if !passed {
        eprintln!(
            "Acceptance criteria FAILED for knowledge stage '{}'",
            stage.id
        );
        print_acceptance_failure_guidance(&stage.id);
        bail!(
            "Acceptance criteria failed for knowledge stage '{}'",
            stage.id
        );
    }
    Ok(())
}
