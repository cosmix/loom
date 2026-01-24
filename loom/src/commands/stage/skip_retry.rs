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

/// Retry a stage that is blocked, completed with failures, or merge-blocked
pub fn retry(stage_id: String, force: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Allow retry for Blocked, CompletedWithFailures, and MergeBlocked states
    let retryable = matches!(
        stage.status,
        StageStatus::Blocked | StageStatus::CompletedWithFailures | StageStatus::MergeBlocked
    );

    if !retryable {
        bail!(
            "Cannot retry stage in status: {}. Only blocked, completed-with-failures, or merge-blocked stages can be retried.",
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

    // Reset or increment for retry
    if force {
        stage.retry_count = 0;
        stage.failure_info = None;
    } else {
        // Increment retry count for non-forced retries
        // This ensures retry limit is enforced for manual retry attempts
        stage.retry_count += 1;
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
