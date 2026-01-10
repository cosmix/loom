//! Stage state transition commands

use anyhow::Result;
use std::path::Path;

use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage};

/// Block a stage with a reason
pub fn block(stage_id: String, reason: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.status = StageStatus::Blocked;
    stage.close_reason = Some(reason.clone());
    stage.updated_at = chrono::Utc::now();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' blocked");
    println!("Reason: {reason}");
    Ok(())
}

/// Reset a stage to pending
pub fn reset(stage_id: String, hard: bool, kill_session: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    // Kill tmux session if requested
    if kill_session {
        let tmux_name = format!("loom-{stage_id}");
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", &tmux_name])
            .output();
    }

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.status = StageStatus::WaitingForDeps;
    stage.completed_at = None;
    stage.close_reason = None;
    stage.updated_at = chrono::Utc::now();

    // Hard reset also clears session assignment
    if hard {
        stage.session = None;
    }

    save_stage(&stage, work_dir)?;

    let mode = if hard { "hard" } else { "soft" };
    println!("Stage '{stage_id}' reset to pending ({mode} reset)");
    Ok(())
}

/// Mark a stage as ready for execution
pub fn ready(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.try_mark_queued()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' marked as ready");
    Ok(())
}

/// Mark a stage as waiting for user input (called by hooks)
pub fn waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently executing
    if stage.status != StageStatus::Executing {
        // Silently skip if not executing - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not executing. Skipping waiting transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.try_mark_waiting_for_input()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' waiting for user input");
    Ok(())
}

/// Resume a stage from waiting for input state (called by hooks)
pub fn resume_from_waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently waiting for input
    if stage.status != StageStatus::WaitingForInput {
        // Silently skip if not waiting - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not waiting. Skipping resume transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.try_mark_executing()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' resumed execution");
    Ok(())
}

/// Hold a stage (prevent auto-execution even when ready)
pub fn hold(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if stage.held {
        println!("Stage '{stage_id}' is already held");
        return Ok(());
    }

    stage.hold();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' held");
    println!("The stage will not auto-execute. Use 'loom stage release {stage_id}' to unlock.");
    Ok(())
}

/// Release a held stage (allow auto-execution)
pub fn release(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if !stage.held {
        println!("Stage '{stage_id}' is not held");
        return Ok(());
    }

    stage.release();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' released");
    Ok(())
}
