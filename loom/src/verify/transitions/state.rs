//! Stage state transitions and dependency triggering
//!
//! This module handles:
//! - Transitioning stages to new statuses
//! - Triggering dependent stages when dependencies are satisfied

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::{branch_ref, is_ancestor_of};
use crate::models::stage::{Stage, StageStatus, StageType};

use super::persistence::{list_all_stages, load_stage, update_stage};

#[path = "dependency_cache.rs"]
mod dependency_cache;
pub use dependency_cache::are_all_dependencies_satisfied_cached;

/// Validate and persist a stage status transition.
pub fn transition_stage(stage_id: &str, new_status: StageStatus, work_dir: &Path) -> Result<Stage> {
    update_stage(stage_id, work_dir, |stage| {
        stage
            .try_transition(new_status.clone())
            .with_context(|| format!("Invalid transition for stage {stage_id}"))?;

        if new_status == StageStatus::Completed {
            stage.completed_at = Some(chrono::Utc::now());
        }
        Ok(())
    })
}

/// Queue waiting dependents when all their prerequisites become final. A
/// dependent that cannot be checked or queued is logged and left waiting for
/// the next poll: the caller has already recorded the merge, so one dependent's
/// failure must not cost its siblings their transition or skip the cleanup
/// that follows.
pub fn trigger_dependents(
    completed_stage_id: &str,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<Vec<String>> {
    let all_stages = list_all_stages(work_dir)?;
    let mut triggered = Vec::new();

    for stage in all_stages {
        if !stage.dependencies.contains(&completed_stage_id.to_string()) {
            continue;
        }

        // Only waiting stages can be queued.
        if stage.status != StageStatus::WaitingForDeps {
            continue;
        }

        match queue_if_satisfied(&stage, work_dir, repo_root, target_branch) {
            Ok(true) => triggered.push(stage.id),
            Ok(false) => {}
            Err(error) => tracing::error!(
                stage_id = %stage.id, completed = %completed_stage_id, error = %format!("{error:#}"),
                "Could not trigger dependent stage; the next poll rechecks it"
            ),
        }
    }

    Ok(triggered)
}

/// Queue `stage` if its dependencies are satisfied. `true` means it moved.
fn queue_if_satisfied(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<bool> {
    if !are_all_dependencies_satisfied(stage, work_dir, repo_root, target_branch)? {
        return Ok(false);
    }
    let mut transitioned = false;
    update_stage(&stage.id, work_dir, |current| {
        // A concurrent amendment or scheduler may have changed the
        // record after the dependency probe. Only apply the exact
        // transition we evaluated; otherwise the next poll rechecks.
        if current.status != StageStatus::WaitingForDeps
            || current.dependencies != stage.dependencies
        {
            return Ok(());
        }
        current.try_mark_queued().with_context(|| {
            format!(
                "Failed to transition stage {} from {:?} to Queued",
                current.id, current.status
            )
        })?;
        transitioned = true;
        Ok(())
    })
    .with_context(|| format!("Failed to update triggered stage: {}", stage.id))?;
    Ok(transitioned)
}

/// Why a stage cannot be scheduled, and whether waiting will fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyBlock {
    /// The dependency that is holding the stage back (the first one found).
    pub dependency: String,
    /// Human-readable explanation, e.g. "Completed but not merged".
    pub detail: String,
    /// `true` when the orchestrator will resolve this on its own (a dep still
    /// running, or completed and awaiting auto-merge). `false` when it needs a
    /// human — the stage will otherwise sit queued indefinitely, which is
    /// exactly the situation that reads as a hang.
    pub self_resolving: bool,
}

enum DependencyReason {
    Status(StageStatus, bool),
    MissingCommit,
    PhantomMerge(String),
}

enum DependencyFailure {
    Read {
        dependency: String,
        error: anyhow::Error,
    },
    Ancestry {
        dependency: String,
        error: anyhow::Error,
    },
}

/// Evaluate dependencies in declared order. Knowledge and assumed merges
/// need no git proof; other final merges must be ancestors of the target.
fn evaluate_dependencies(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> std::result::Result<Option<(String, DependencyReason)>, DependencyFailure> {
    for dep_id in &stage.dependencies {
        let dep = load_stage(dep_id, work_dir).map_err(|error| DependencyFailure::Read {
            dependency: dep_id.clone(),
            error,
        })?;
        if !dep.is_final_merged() {
            let status = dep.status;
            let resolving = matches!(
                &status,
                StageStatus::WaitingForDeps
                    | StageStatus::Queued
                    | StageStatus::Executing
                    | StageStatus::NeedsHandoff
                    | StageStatus::WaitingForInput
            ) || status == StageStatus::Completed;
            return Ok(Some((
                dep_id.clone(),
                DependencyReason::Status(status, resolving),
            )));
        }
        if dep.stage_type == StageType::Knowledge || dep.merge_assumed {
            continue;
        }
        let Some(commit) = dep.completed_commit else {
            return Ok(Some((dep_id.clone(), DependencyReason::MissingCommit)));
        };
        match is_ancestor_of(&commit, &branch_ref(target_branch), repo_root) {
            Ok(true) => {}
            Ok(false) => {
                return Ok(Some((
                    dep_id.clone(),
                    DependencyReason::PhantomMerge(commit),
                )));
            }
            Err(error) => {
                return Err(DependencyFailure::Ancestry {
                    dependency: dep_id.clone(),
                    error,
                });
            }
        }
    }
    Ok(None)
}

fn status_detail(status: StageStatus) -> String {
    match status {
        StageStatus::Completed => "Completed but not merged yet".to_string(),
        StageStatus::MergeConflict | StageStatus::MergeBlocked => {
            format!("{status:?} — needs merge resolution")
        }
        StageStatus::Blocked | StageStatus::CompletedWithFailures => {
            format!("{status:?} — retry or fix it to unblock this stage")
        }
        StageStatus::NeedsHumanReview | StageStatus::NeedsAdjudication => {
            format!("{status:?} — awaiting a verdict")
        }
        StageStatus::Skipped => "Skipped — dependents can never become ready; skip them too or \
                 re-run the dependency"
            .to_string(),
        _ => format!("still {status:?}"),
    }
}

/// Explain the first dependency blocking this stage. An operational error is
/// displayed as a block here, while the scheduling predicate returns it so a
/// transient failure is retried without entering the cache.
pub fn describe_dependency_block(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<Option<DependencyBlock>> {
    let block = match evaluate_dependencies(stage, work_dir, repo_root, target_branch) {
        Ok(None) => return Ok(None),
        Ok(Some((dependency, reason))) => {
            let (detail, self_resolving) = match reason {
                DependencyReason::Status(status, resolving) => (status_detail(status), resolving),
                DependencyReason::MissingCommit => (
                    "marked merged but has no completed_commit, so the merge cannot be verified"
                        .to_string(),
                    false,
                ),
                DependencyReason::PhantomMerge(commit) => (
                    format!(
                        "marked merged but commit {} is not in {} (phantom merge)",
                        &commit[..commit.len().min(8)],
                        target_branch
                    ),
                    false,
                ),
            };
            DependencyBlock {
                dependency,
                detail,
                self_resolving,
            }
        }
        Err(DependencyFailure::Read { dependency, error }) => DependencyBlock {
            dependency,
            detail: format!("stage file could not be read ({error})"),
            self_resolving: false,
        },
        Err(DependencyFailure::Ancestry { dependency, error }) => DependencyBlock {
            dependency,
            detail: format!("merge ancestry check failed ({error})"),
            self_resolving: false,
        },
    };
    Ok(Some(block))
}

/// A dependency must be completed and merged. Knowledge and operator-assumed
/// merges need no git ancestry proof; other merges require their completed
/// commit on the target branch. Operational errors retry instead of becoming
/// cached blocks.
pub fn are_all_dependencies_satisfied(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<bool> {
    match evaluate_dependencies(stage, work_dir, repo_root, target_branch) {
        Ok(None) => Ok(true),
        Ok(Some((dependency, reason))) => {
            match reason {
                DependencyReason::MissingCommit => tracing::error!(
                    stage_id = %stage.id, dep_id = %dependency,
                    "Dependency marked merged but has no completed_commit — cannot verify ancestry"
                ),
                DependencyReason::PhantomMerge(commit) => tracing::error!(
                    stage_id = %stage.id, dep_id = %dependency, commit = %commit,
                    target = %target_branch,
                    "Phantom merge detected: dependency commit not in target branch"
                ),
                DependencyReason::Status(..) => {}
            }
            Ok(false)
        }
        Err(DependencyFailure::Read { dependency, error }) => Err(error).with_context(|| {
            format!(
                "Failed to load dependency stage {dependency} for stage {}",
                stage.id
            )
        }),
        Err(DependencyFailure::Ancestry { dependency, error }) => {
            tracing::error!(
                stage_id = %stage.id, dep_id = %dependency, error = %error,
                "Git ancestry check failed — refusing to satisfy dependency"
            );
            Err(error)
        }
    }
}
