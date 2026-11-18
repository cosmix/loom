//! Abandoning an unfinished run (section 11, "Abandoning an unfinished run").
//!
//! The sequence stops the daemon, runs `loom clean --all`, confirms on disk that the run is
//! gone, then renames `<plan home>/IN_PROGRESS-PLAN-<slug>.md` to
//! `<plan home>/archive/RETIRED-PLAN-<slug>.md` through the no-follow primitives. The plan home
//! is `doc/plans/` for a standalone plan and `doc/plans/<feature>/` for a feature plan. An abandon
//! marker in the server's runtime directory records that an abandon began here, so a server
//! that stopped between `loom clean` and the rename can offer "Finish abandoning".

mod marker;
mod plan;
mod retire;
mod route;
mod sequence;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::commands::clean::{unmerged_loom_branches, UnmergedBranches};
use crate::fs::work_dir::WorkDir;
use crate::plan_server::context::PlanContext;
use crate::plan_server::routes::RouteSpec;
use crate::plan_server::wire::run::{AbandonView, UnmergedBranch};
use crate::planning::runtime::RuntimePaths;
use crate::status::data::collect_status_data;

/// How an abandon or a finish-abandon ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbandonOutcome {
    /// The plan moved to `to` (repository-relative).
    Renamed { to: PathBuf },
    /// A check refused the step; the plan keeps its name.
    Refused { reason: String },
    /// The unmerged `loom/*` branches differ from the confirmed list; `now` is the fresh list.
    BranchesChanged { now: Vec<(String, usize)> },
    /// A child exited non-zero (`-1` when it could not run or was killed).
    Failed { step: String, code: i32 },
}

/// The `POST /api/run/abandon` and `POST /api/run/finish-abandon` routes.
pub(super) fn routes() -> Vec<RouteSpec> {
    route::routes()
}

/// Run the abandon sequence for the workspace's `IN_PROGRESS-` plan. `loom_bin` is the `loom`
/// executable the children run; `confirmed_unmerged` is the (branch, ahead) list the holder
/// confirmed, empty when none was shown. `on_line` receives every output line of the children
/// and of the sequence's own checks.
pub fn run_abandon(
    root: &Path,
    runtime: &RuntimePaths,
    loom_bin: &Path,
    confirmed_unmerged: &[(String, usize)],
    on_line: &mut dyn FnMut(&str),
) -> anyhow::Result<AbandonOutcome> {
    sequence::run(root, runtime, loom_bin, confirmed_unmerged, on_line)
}

/// Rename a stranded plan (see `stranded_plan`) alone; anything of the run left on disk, or
/// a missing abandon marker, refuses it.
pub fn finish_abandon(root: &Path, runtime: &RuntimePaths) -> anyhow::Result<AbandonOutcome> {
    match retire::stranded(root, runtime) {
        Ok(plan) => retire::retire_step(root, runtime, &plan, &mut |_| {}),
        Err(reason) => Ok(AbandonOutcome::Refused { reason }),
    }
}

/// The repository-relative `IN_PROGRESS-` plan an interrupted abandon left: an abandon marker
/// in `runtime` names it, and no workspace, `loom/*` branch or `.worktrees/*` directory
/// remains. Without the marker it is `None`: a plan running from another clone looks the same.
pub(crate) fn stranded_plan(root: &Path, runtime: &RuntimePaths) -> Option<String> {
    retire::stranded(root, runtime)
        .ok()
        .map(|plan| plan.source_string())
}

/// What the abandon dialog shows, when the workspace's plan is `IN_PROGRESS-`. When the
/// `loom/*` branches cannot be listed the list is empty and the abandon request itself refuses
/// with the reason (the view has no refusal field to carry it).
pub(crate) fn view(ctx: &PlanContext) -> Option<AbandonView> {
    let root = ctx.root();
    let plan = plan::running_plan(root)?;
    let (merged_stages, unmerged_stages) = stage_split(root);
    let UnmergedBranches {
        target_branch,
        branches,
        listing_error,
    } = unmerged_loom_branches(root);
    if let Some(error) = listing_error {
        tracing::warn!("abandon view: the loom/* branches cannot be listed: {error}");
    }
    Some(AbandonView {
        plan: plan.source_string(),
        merged_stages,
        unmerged_stages,
        unmerged_branches: wire_branches(branches),
        base_branch: target_branch,
    })
}

/// (branch, ahead) pairs as wire values.
fn wire_branches(branches: Vec<(String, usize)>) -> Vec<UnmergedBranch> {
    branches
        .into_iter()
        .map(|(branch, ahead)| UnmergedBranch {
            branch,
            ahead: u32::try_from(ahead).unwrap_or(u32::MAX),
        })
        .collect()
}

/// The run's merged stage ids and every other stage id; both empty when the status cannot be
/// read (the dialog still offers the abandon).
fn stage_split(root: &Path) -> (Vec<String>, Vec<String>) {
    let data = WorkDir::new(root).and_then(|work_dir| collect_status_data(&work_dir));
    match data {
        Ok(data) => {
            let merged = data.merge.merged;
            let unmerged = data
                .stages
                .iter()
                .map(|stage| stage.id.clone())
                .filter(|id| !merged.contains(id))
                .collect();
            (merged, unmerged)
        }
        Err(error) => {
            tracing::warn!("abandon view: the run's stages could not be read: {error:#}");
            (Vec::new(), Vec::new())
        }
    }
}
