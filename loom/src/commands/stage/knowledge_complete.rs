//! Knowledge stage completion logic
//!
//! Handles completion for knowledge stages which run in the main repo context
//! (no worktree) and update documentation in `doc/loom/knowledge/`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::models::stage::StageType;
use crate::verify::criteria::run_acceptance;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

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
) -> Result<()> {
    let work_dir = Path::new(".work");
    let mut stage = load_stage(stage_id, work_dir)?;

    // Verify this is actually a knowledge stage
    debug_assert!(
        stage.stage_type == StageType::Knowledge,
        "complete_knowledge_stage called on non-knowledge stage"
    );

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
    let triggered =
        trigger_dependents(stage_id, work_dir).context("Failed to trigger dependent stages")?;

    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  → {dep_id}");
        }
    }

    Ok(())
}
