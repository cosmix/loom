//! Stage state transitions and dependency triggering
//!
//! This module handles:
//! - Transitioning stages to new statuses
//! - Triggering dependent stages when dependencies are satisfied

use anyhow::{Context, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use crate::git::branch::is_ancestor_of;
use crate::models::stage::{Stage, StageStatus, StageType};

use super::persistence::{list_all_stages, load_stage, save_stage};

/// How long a cached dependency-satisfaction result stays valid before the
/// (potentially expensive) git-ancestry check is re-run, even if no dependency
/// stage file changed. Bounds staleness so a manually-edited git history is
/// noticed within a minute.
const DEP_CHECK_RECHECK_INTERVAL: Duration = Duration::from_secs(60);

/// One cached `are_all_dependencies_satisfied` result for a stage.
struct DepCheckCacheEntry {
    /// When the underlying check last actually ran.
    checked_at: Instant,
    /// Modification times of every dependency stage file at check time, in
    /// dependency order. A change here means a dependency advanced (e.g. became
    /// merged) so the check must re-run.
    dep_fingerprint: Vec<Option<SystemTime>>,
    /// The result the expensive check returned (always `Ok(bool)` — errors are
    /// never cached so a transient failure is retried next tick).
    result: bool,
}

thread_local! {
    /// Per-stage cache of dependency-satisfaction results, used by
    /// [`are_all_dependencies_satisfied_cached`]. Lives on the orchestrator's
    /// (single) poll thread; never shared across threads.
    static DEP_CHECK_CACHE: RefCell<HashMap<String, DepCheckCacheEntry>> =
        RefCell::new(HashMap::new());
}

/// Fingerprint a stage's dependency files by their modification times.
///
/// Returns one entry per dependency (in order); `None` for a dep whose file is
/// missing or whose mtime can't be read. A change in this vector indicates a
/// dependency stage file was rewritten (status/merged flag advanced), which is
/// the signal to re-run the satisfaction check.
fn dependency_mtime_fingerprint(stage: &Stage, work_dir: &Path) -> Vec<Option<SystemTime>> {
    let stages_dir = work_dir.join("stages");
    stage
        .dependencies
        .iter()
        .map(|dep_id| {
            crate::fs::stage_files::find_stage_file(&stages_dir, dep_id)
                .ok()
                .flatten()
                .and_then(|path| path.metadata().ok())
                .and_then(|meta| meta.modified().ok())
        })
        .collect()
}

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

/// Cached form of [`are_all_dependencies_satisfied`] for the per-tick spawn guard.
///
/// The spawn-time phantom-merge guard runs on every orchestrator poll for every
/// not-yet-ready stage. The underlying check loads each dependency stage file
/// and shells out to `git merge-base --is-ancestor` per non-knowledge dep — work
/// that is wasted when nothing changed since last tick (P-6).
///
/// This wrapper memoizes the *boolean* result per stage and only re-runs the
/// expensive check when either:
///   - a dependency stage file's mtime changed (a dep advanced, e.g. merged), or
///   - more than [`DEP_CHECK_RECHECK_INTERVAL`] elapsed since the last real run
///     (bounds staleness against out-of-band git history changes).
///
/// Errors are never cached: a transient git failure returns `Err` and leaves the
/// cache untouched so the next tick retries.
pub fn are_all_dependencies_satisfied_cached(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<bool> {
    // No deps → trivially satisfied, nothing to cache.
    if stage.dependencies.is_empty() {
        return Ok(true);
    }

    let fingerprint = dependency_mtime_fingerprint(stage, work_dir);
    let now = Instant::now();

    // Fast path: reuse the cached result if the dependency files are unchanged
    // and the recheck interval has not elapsed.
    let cached = DEP_CHECK_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache.get(&stage.id).and_then(|entry| {
            let fresh = now.duration_since(entry.checked_at) < DEP_CHECK_RECHECK_INTERVAL;
            if fresh && entry.dep_fingerprint == fingerprint {
                Some(entry.result)
            } else {
                None
            }
        })
    });
    if let Some(result) = cached {
        return Ok(result);
    }

    // Slow path: run the real check and cache the boolean outcome.
    let result = are_all_dependencies_satisfied(stage, work_dir, repo_root, target_branch)?;
    DEP_CHECK_CACHE.with(|cache| {
        cache.borrow_mut().insert(
            stage.id.clone(),
            DepCheckCacheEntry {
                checked_at: now,
                dep_fingerprint: fingerprint,
                result,
            },
        );
    });
    Ok(result)
}
