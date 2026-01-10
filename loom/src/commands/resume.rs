use crate::fs::work_dir::WorkDir;
use crate::handoff::{
    continue_session, find_latest_handoff, load_handoff_content, prepare_continuation,
    ContinuationConfig,
};
use crate::models::stage::StageStatus;
use crate::models::worktree::Worktree;
use crate::verify::transitions::{load_stage, save_stage};
use anyhow::{bail, Context, Result};
use std::io::{stdin, stdout, Write};

/// Resume failed/blocked stages with handoff context
/// Usage: loom resume <stage_id>
pub fn execute(stage_id: String) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let mut stage = load_stage(&stage_id, work_dir.root())?;

    if !matches!(
        stage.status,
        StageStatus::Blocked | StageStatus::NeedsHandoff
    ) {
        bail!(
            "Stage '{}' has status {:?}. Can only resume stages with status Blocked or NeedsHandoff.",
            stage_id,
            stage.status
        );
    }

    println!("Stage: {} (status: {:?})", stage.name, stage.status);

    let handoff_path = find_latest_handoff(&stage_id, work_dir.root())?;

    if let Some(ref path) = handoff_path {
        println!("\nLatest handoff: {}", path.display());
        let content = load_handoff_content(path)?;
        let lines: Vec<&str> = content.lines().take(20).collect();
        println!("\nHandoff summary (first 20 lines):");
        println!("---");
        for line in lines {
            println!("{line}");
        }
        println!("---");
    } else {
        println!("\nNo handoff found for this stage.");
    }

    print!("\nResume this stage? (y/n): ");
    stdout().flush()?;

    let mut response = String::new();
    stdin().read_line(&mut response)?;

    if !response.trim().eq_ignore_ascii_case("y") {
        println!("Resume cancelled.");
        return Ok(());
    }

    // Prepare continuation context
    let context = prepare_continuation(&stage_id, work_dir.root())
        .context("Failed to prepare continuation context")?;

    // Create continuation configuration with auto_spawn enabled
    let config = ContinuationConfig {
        auto_spawn: true,
        ..Default::default()
    };

    // Check if we have a worktree to spawn the session in
    if let Some(worktree_id) = &stage.worktree {
        // Create worktree object for the continuation
        let worktree = Worktree::new(
            worktree_id.clone(),
            context.worktree_path.clone(),
            context.branch.clone(),
        );

        // Spawn the tmux session with handoff context
        let session = continue_session(
            &context.stage,
            context.handoff_path.as_deref(),
            &worktree,
            &config,
            work_dir.root(),
        )
        .context("Failed to spawn continuation session")?;

        // Update stage status to Executing
        stage.try_mark_executing()?;
        save_stage(&stage, work_dir.root())?;

        println!("\n✓ Stage status updated to Executing");
        println!(
            "✓ Spawned tmux session: {}",
            session.tmux_session.as_ref().unwrap_or(&"none".to_string())
        );
        println!("\nTo attach to the session:");
        println!(
            "  tmux attach -t {}",
            session.tmux_session.as_ref().unwrap_or(&"none".to_string())
        );

        if let Some(ref path) = handoff_path {
            println!("\nHandoff context loaded from: {}", path.display());
        }
    } else {
        // No worktree assigned - just update the status
        stage.try_mark_executing()?;
        save_stage(&stage, work_dir.root())?;

        println!("\n✓ Stage status updated to Executing");
        println!("\n⚠️  No worktree assigned to this stage.");
        println!("Work can be resumed manually in the main directory.");

        if let Some(ref path) = handoff_path {
            println!("\nHandoff available at: {}", path.display());
        }
    }

    Ok(())
}
