//! Stage state transitions and dependency triggering
//!
//! This module handles:
//! - Transitioning stages to new statuses
//! - Triggering dependent stages when dependencies are satisfied

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::is_ancestor_of;
use crate::models::stage::{Stage, StageStatus, StageType};

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
/// * `repo_root` - Path to the git repository root (used for ancestry verification)
/// * `target_branch` - The merge target branch (e.g., `main`) used for git ancestry checks
///
/// # Returns
/// List of stage IDs that were transitioned to Ready
///
/// # Note
/// Only stages in `Pending` status are eligible for triggering, which is
/// a valid transition to `Ready` per the state machine.
pub fn trigger_dependents(
    completed_stage_id: &str,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<Vec<String>> {
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

        if are_all_dependencies_satisfied(&stage, work_dir, repo_root, target_branch)? {
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
/// A dependency is satisfied if:
///   - Its status is `Completed`, AND
///   - `merged` is true, AND
///   - (non-knowledge deps) its `completed_commit` is an ancestor of the target branch.
///
/// Knowledge stages are exempt from the git ancestry check because they
/// have no branch by design — their "merge" is a pure metadata operation.
///
/// The git ancestry check is defense-in-depth against phantom merges: the
/// `merged` flag can lie (see `PLAN-fix-phantom-merge.md`), so we
/// cross-check that the dep's commit actually lives in the target branch.
///
/// # Arguments
/// * `stage` - The stage whose dependencies we are checking
/// * `work_dir` - Path to the `.work` directory
/// * `repo_root` - Path to the git repository root (used for ancestry verification)
/// * `target_branch` - The merge target branch (e.g., `main`)
///
/// # Returns
/// `true` if all dependencies are satisfied per the rules above, `false` otherwise.
pub fn are_all_dependencies_satisfied(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<bool> {
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

        // Knowledge stages are exempt from the git ancestry check
        // (they have no branch by design). Status + merged flag are sufficient.
        if dep_stage.stage_type == StageType::Knowledge {
            if dep_stage.status != StageStatus::Completed || !dep_stage.merged {
                return Ok(false);
            }
            continue;
        }

        // Non-knowledge stages: require status, merged flag, AND git ancestry.
        if dep_stage.status != StageStatus::Completed || !dep_stage.merged {
            return Ok(false);
        }

        let Some(ref completed_commit) = dep_stage.completed_commit else {
            tracing::error!(
                stage_id = %stage.id,
                dep_id = %dep_id,
                "Dependency marked merged but has no completed_commit — cannot verify ancestry"
            );
            return Ok(false);
        };

        match is_ancestor_of(completed_commit, target_branch, repo_root) {
            Ok(true) => continue,
            Ok(false) => {
                tracing::error!(
                    stage_id = %stage.id,
                    dep_id = %dep_id,
                    commit = %completed_commit,
                    target = %target_branch,
                    "Phantom merge detected: dependency commit not in target branch — refusing to satisfy dependency"
                );
                return Ok(false);
            }
            Err(e) => {
                tracing::error!(
                    stage_id = %stage.id,
                    dep_id = %dep_id,
                    error = %e,
                    "Git ancestry check failed — refusing to satisfy dependency"
                );
                return Ok(false);
            }
        }
    }

    Ok(true)
}
