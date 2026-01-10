//! Skip and retry commands for stages

use anyhow::{bail, Result};
use std::path::Path;

use crate::models::stage::StageStatus;
use crate::orchestrator::skip::skip_stage;
use crate::verify::transitions::{load_stage, save_stage};

/// Skip a stage
pub fn skip(stage_id: String, reason: Option<String>) -> Result<()> {
    let work_dir = Path::new(".work");

    skip_stage(&stage_id, reason.clone(), work_dir)?;

    println!("Stage '{stage_id}' skipped.");
    if let Some(r) = reason {
        println!("Reason: {r}");
    }
    println!("Note: Dependent stages will remain blocked.");

    Ok(())
}

/// Retry a blocked stage
pub fn retry(stage_id: String, force: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if stage.status != StageStatus::Blocked {
        bail!(
            "Cannot retry stage in status: {}. Only blocked stages can be retried.",
            stage.status
        );
    }

    let max = stage.max_retries.unwrap_or(3);
    if !force && stage.retry_count >= max {
        bail!(
            "Stage '{}' has exceeded retry limit ({}/{}). Use --force to override.",
            stage_id,
            stage.retry_count,
            max
        );
    }

    // Reset for retry
    if force {
        stage.retry_count = 0;
        stage.failure_info = None;
    }
    stage.last_failure_at = None;
    stage.try_mark_queued()?;

    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' queued for retry.");
    if force {
        println!("Retry count reset (--force used).");
    }

    Ok(())
}
