use crate::fs::work_dir::WorkDir;
use crate::handoff::{find_latest_handoff, load_handoff_content};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage};
use anyhow::{bail, Result};
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

    stage.try_mark_executing()?;
    save_stage(&stage, work_dir.root())?;

    println!("\nStage status updated to Executing");

    if let Some(worktree) = &stage.worktree {
        let project_root = work_dir.root().parent().unwrap_or(work_dir.root());
        let worktree_path = project_root.join(".worktrees").join(worktree);
        println!("\nManual session start required:");
        println!("  cd {}", worktree_path.display());
        if let Some(ref path) = handoff_path {
            println!("  # Review handoff: {}", path.display());
        }
        println!("  # Continue work on stage");
    } else {
        println!("\nNo worktree assigned to this stage.");
        println!("Work can be resumed in the main directory.");
    }

    Ok(())
}
