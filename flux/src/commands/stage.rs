//! Stage state manipulation
//! Usage: flux stage <id> [complete|block|reset|ready]

use anyhow::Result;
use std::path::Path;

use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage};

/// Mark a stage as complete
pub fn complete(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.complete(None);
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' marked as complete");
    Ok(())
}

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
pub fn reset(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.status = StageStatus::Pending;
    stage.completed_at = None;
    stage.close_reason = None;
    stage.updated_at = chrono::Utc::now();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' reset to pending");
    Ok(())
}

/// Mark a stage as ready for execution
pub fn ready(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.mark_ready();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' marked as ready");
    Ok(())
}
