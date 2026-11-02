//! Base branch resolution for worktree creation
//!
//! Determines the correct base branch for a new worktree based on stage dependencies.
//! With progressive merge, all dependency work is merged to main before dependent stages
//! can be scheduled, so the base is always main (or a single unmerged dependency branch).

use std::fmt;
use std::path::Path;

use crate::git::branch::{branch_exists, branch_name_for_stage, resolve_target_branch};
use crate::models::stage::StageStatus;
use crate::plan::graph::ExecutionGraph;

/// Typed failure modes for base-branch resolution.
///
/// Callers (notably the stage executor) route on the *variant* rather than
/// matching substrings of an error message, so rewording a message can never
/// silently reclassify a handled condition into a propagated error.
#[derive(Debug)]
pub enum BaseBranchError {
    /// Dependencies are not yet ready (not completed/merged) — a transient
    /// condition that should be retried on the next poll cycle, not an error.
    SchedulingNotReady(String),
    /// Any other failure (e.g. a git command failing, a dependency missing
    /// from the graph) — propagate to the caller.
    Other(anyhow::Error),
}

impl fmt::Display for BaseBranchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BaseBranchError::SchedulingNotReady(msg) => write!(f, "{msg}"),
            BaseBranchError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BaseBranchError {}

impl From<anyhow::Error> for BaseBranchError {
    fn from(e: anyhow::Error) -> Self {
        BaseBranchError::Other(e)
    }
}

/// Result of resolving the base branch for a stage
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBase {
    /// Use the main/master branch as base (merge point containing all dep work)
    Main(String),
    /// Use a specific loom branch as base (single unmerged dependency)
    Branch(String),
}

impl ResolvedBase {
    /// Get the branch name to use
    pub fn branch_name(&self) -> &str {
        match self {
            ResolvedBase::Main(name) => name,
            ResolvedBase::Branch(name) => name,
        }
    }
}

/// Resolve the base branch for a stage based on its dependencies.
///
/// # Logic
///
/// With progressive merge, stages are only scheduled when all dependencies are
/// `Completed` AND `merged: true`. This means all dependency work is already
/// in main, so we can use main as the base.
///
/// - No deps → Use init_base_branch if provided, else default_branch (main)
/// - All deps merged → Use main (contains all dependency work)
/// - Single dep not merged → Use that dep's branch (legacy/fallback path)
/// - Multiple deps, any not merged → Scheduling error (should not happen)
///
/// # Arguments
///
/// * `stage_id` - The ID of the stage needing a worktree
/// * `dependencies` - The stage's dependency list
/// * `graph` - The execution graph (for checking dependency status and merged state)
/// * `repo_root` - Path to the git repository root
/// * `init_base_branch` - Optional base branch from config.toml (for stages with no deps)
///
/// # Returns
///
/// * `Ok(ResolvedBase)` - The resolved base branch to use
/// * `Err(BaseBranchError)` - A typed failure: `SchedulingNotReady` (retry
///   next cycle), or `Other` (propagate).
pub fn resolve_base_branch(
    stage_id: &str,
    dependencies: &[String],
    graph: &ExecutionGraph,
    repo_root: &Path,
    init_base_branch: Option<&str>,
) -> Result<ResolvedBase, BaseBranchError> {
    tracing::debug!(
        stage_id,
        ?dependencies,
        ?init_base_branch,
        "resolving base branch"
    );

    let target = resolve_target_branch(&init_base_branch.map(String::from), repo_root);

    // No deps → use init_base_branch if provided, otherwise fall back to default
    if dependencies.is_empty() {
        tracing::debug!(target, "no deps, using base");
        return Ok(ResolvedBase::Main(target));
    }

    // Check all dependencies are completed and merged
    let mut unmerged_deps: Vec<(&str, bool, bool)> = Vec::new(); // (id, completed, merged)

    for dep in dependencies {
        let dep_node = graph
            .get_node(dep)
            .ok_or_else(|| anyhow::anyhow!("Dependency '{dep}' not found in graph"))?;

        let is_completed = dep_node.status == StageStatus::Completed;
        let is_merged = dep_node.merged;

        tracing::debug!(dep, status = ?dep_node.status, is_completed, is_merged, "checking dependency");

        if !is_completed || !is_merged {
            unmerged_deps.push((dep.as_str(), is_completed, is_merged));
        }
    }

    // All deps completed and merged - use merge point (init_base_branch or default)
    if unmerged_deps.is_empty() {
        tracing::debug!(target, "all deps merged, using base");
        return Ok(ResolvedBase::Main(target));
    }

    // Single dependency that's completed but not merged - use its branch directly
    // (This handles the edge case during initial stages or when progressive merge is disabled)
    if dependencies.len() == 1 {
        let (dep, is_completed, is_merged) = unmerged_deps[0];

        if is_completed && !is_merged {
            // Dep is done but not merged yet - check if branch exists
            let dep_branch = branch_name_for_stage(dep);
            if branch_exists(&dep_branch, repo_root)? {
                return Ok(ResolvedBase::Branch(dep_branch));
            }
            // Recovery fallback: a missing branch does not prove its work was merged.
            tracing::warn!(
                dep,
                target,
                "dependency branch missing; assuming its work is in the target"
            );
            return Ok(ResolvedBase::Main(target));
        }

        // Dependency not completed
        return Err(BaseBranchError::SchedulingNotReady(format!(
            "Scheduling error: dependency '{}' is {:?} (completed={}, merged={}). \
             Stages should only be scheduled after their dependencies complete and merge.",
            dep,
            graph.get_node(dep).map(|n| &n.status),
            is_completed,
            is_merged
        )));
    }

    // Multiple dependencies with some not ready - scheduling error
    let not_ready: Vec<_> = unmerged_deps
        .iter()
        .map(|(id, completed, merged)| format!("{id}(completed={completed}, merged={merged})"))
        .collect();

    Err(BaseBranchError::SchedulingNotReady(format!(
        "Scheduling error: stage '{}' has dependencies not ready: [{}]. \
         All dependencies must be completed AND merged before scheduling.",
        stage_id,
        not_ready.join(", ")
    )))
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;
