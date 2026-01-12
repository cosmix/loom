//! Stage state transitions and dependency triggering
//!
//! This module handles:
//! - Transitioning stages to new statuses
//! - Triggering dependent stages when dependencies are satisfied

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::stage::{Stage, StageStatus};

use super::persistence::{list_all_stages, load_stage, save_stage};

/// Transition a stage to a new status with validation
///
/// Loads the stage from `.work/stages/{stage_id}.md`, validates and updates
/// its status using validated transition methods, saves it back to disk,
/// and returns the updated stage.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to transition
/// * `new_status` - The new status to assign
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// The updated stage, or an error if the transition is invalid
///
/// # Errors
/// Returns an error if:
/// - The stage cannot be loaded
/// - The transition is invalid (e.g., `Verified` -> `Pending`)
/// - The stage cannot be saved
pub fn transition_stage(stage_id: &str, new_status: StageStatus, work_dir: &Path) -> Result<Stage> {
    let mut stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage: {stage_id}"))?;

    // Validate and perform the transition
    stage
        .try_transition(new_status.clone())
        .with_context(|| format!("Invalid transition for stage {stage_id}"))?;

    // Handle special case for Completed which sets additional fields
    if new_status == StageStatus::Completed {
        stage.completed_at = Some(chrono::Utc::now());
    }

    save_stage(&stage, work_dir).with_context(|| format!("Failed to save stage: {stage_id}"))?;

    Ok(stage)
}

/// Trigger dependent stages when a stage is completed
///
/// Finds all stages that depend on `completed_stage_id` and checks if all
/// their dependencies are now satisfied (in Completed status). If so, marks
/// them as Ready using validated transitions.
///
/// # Arguments
/// * `completed_stage_id` - The ID of the stage that was just completed
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// List of stage IDs that were transitioned to Ready
///
/// # Note
/// Only stages in `Pending` status are eligible for triggering, which is
/// a valid transition to `Ready` per the state machine.
pub fn trigger_dependents(completed_stage_id: &str, work_dir: &Path) -> Result<Vec<String>> {
    let all_stages = list_all_stages(work_dir)?;
    let mut triggered = Vec::new();

    for mut stage in all_stages {
        if !stage.dependencies.contains(&completed_stage_id.to_string()) {
            continue;
        }

        // Only Pending stages can be triggered to Ready
        if stage.status != StageStatus::WaitingForDeps {
            continue;
        }

        if are_all_dependencies_satisfied(&stage, work_dir)? {
            // Use validated transition - Pending -> Ready is always valid
            stage.try_mark_queued().with_context(|| {
                format!(
                    "Failed to transition stage {} from {:?} to Ready",
                    stage.id, stage.status
                )
            })?;
            let stage_id = &stage.id;
            save_stage(&stage, work_dir)
                .with_context(|| format!("Failed to save triggered stage: {stage_id}"))?;
            triggered.push(stage.id.clone());
        }
    }

    Ok(triggered)
}

/// Check if all dependencies of a stage are satisfied
///
/// A dependency is satisfied if its status is Completed AND merged is true.
/// This ensures dependent stages can use main as their base, containing all
/// dependency work.
///
/// # Arguments
/// * `stage` - The stage to check dependencies for
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// `true` if all dependencies are Completed with merged=true, `false` otherwise
pub(crate) fn are_all_dependencies_satisfied(stage: &Stage, work_dir: &Path) -> Result<bool> {
    if stage.dependencies.is_empty() {
        return Ok(true);
    }

    for dep_id in &stage.dependencies {
        let dep_stage = load_stage(dep_id, work_dir).with_context(|| {
            format!(
                "Failed to load dependency stage {} for stage {}",
                dep_id, stage.id
            )
        })?;

        if dep_stage.status != StageStatus::Completed {
            return Ok(false);
        }
        if !dep_stage.merged {
            return Ok(false);
        }
    }

    Ok(true)
}
