//! Workspace state and the viewer-safe run dialog.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

use crate::commands::init::holds_orchestration_state;
use crate::daemon::DaemonServer;
use crate::fs::plan_lifecycle::get_plan_source_path;
use crate::fs::work_dir::WorkDir;
use crate::git::current_branch;
use crate::plan_server::browser::{decide, EnvView, OpenDecision};
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::run::{Backend, PlanChoice, RunOffer, RunStateView, WorkspaceState};
use crate::planning::project::Project;
use crate::planning::slug::{PlanEntry, PlanState};

/// Classify the workspace by its configured plan's filename and daemon liveness.
pub fn workspace_state(root: &Path) -> anyhow::Result<(WorkspaceState, RunOffer, Option<PathBuf>)> {
    let work_dir = WorkDir::new(root)?;
    if !work_dir.root().exists() || !holds_orchestration_state(work_dir.root()) {
        return Ok((WorkspaceState::Absent, RunOffer::Start, None));
    }
    let source =
        get_plan_source_path(&work_dir)?.context("the workspace does not name a source plan")?;
    let plan = source.strip_prefix(root).unwrap_or(&source).to_path_buf();
    let name = plan
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let (workspace, offer) = if name.starts_with("PLAN-") {
        (WorkspaceState::Initialized, RunOffer::StartInitialized)
    } else if name.starts_with("IN_PROGRESS-") {
        if DaemonServer::is_running(work_dir.root()) {
            (WorkspaceState::Running, RunOffer::NoneRunning)
        } else {
            (WorkspaceState::Stopped, RunOffer::Resume)
        }
    } else if name.starts_with("DONE-") {
        (WorkspaceState::Done, RunOffer::StartNext)
    } else {
        bail!("the workspace source plan has an unknown lifecycle prefix");
    };
    Ok((workspace, offer, Some(plan)))
}

/// Every field is safe to return to a viewer, including the dashboard URL.
pub(crate) fn state_view(ctx: &PlanContext) -> anyhow::Result<RunStateView> {
    let root = ctx.root();
    let (workspace, offer, plan) = workspace_state(root)?;
    let active_plan = plan.map(|path| path.to_string_lossy().into_owned());
    let plans = written_plans(root);
    let preselected = if workspace == WorkspaceState::Initialized {
        active_plan.clone()
    } else {
        plans.first().map(|plan| plan.path.clone())
    };
    let (doc_changes, blocking_changes) = super::changes::split_changes(root)?;
    let abandon = matches!(workspace, WorkspaceState::Running | WorkspaceState::Stopped)
        .then(|| super::abandon::view(ctx))
        .flatten();
    let finish_abandon = (workspace == WorkspaceState::Absent)
        .then(|| super::abandon::stranded_plan(root, ctx.runtime()))
        .flatten();
    Ok(RunStateView {
        clean_notice: workspace == WorkspaceState::Done,
        workspace,
        active_plan,
        offer,
        plans,
        preselected,
        branch: current_branch(root)?,
        doc_changes,
        blocking_changes,
        backends: backends(&EnvView::current()),
        default_backend: Backend::Tmux,
        abandon,
        finish_abandon,
        dashboard_url: super::dashboard::current_url(ctx)
            .map(|url| super::dashboard_parse::strip_token(&url)),
        busy: super::child::jobs(ctx).current(),
    })
}

fn backends(env: &EnvView) -> Vec<Backend> {
    let mut result = vec![Backend::Tmux];
    if decide(env, false) == OpenDecision::Open {
        result.push(Backend::Native);
    }
    result
}

/// Metaplan heading order first, followed by unlisted written plans by filename.
pub(crate) fn written_plans(root: &Path) -> Vec<PlanChoice> {
    match Project::load(root) {
        Ok(project) => project_plans(&project),
        Err(error) => {
            tracing::warn!("run dialog could not discover written plans: {error:#}");
            Vec::new()
        }
    }
}

fn project_plans(project: &Project) -> Vec<PlanChoice> {
    let mut plans = Vec::new();
    let mut listed = BTreeSet::new();
    for (_, entries) in &project.metaplans {
        for entry in entries {
            let Some(plan) = entry
                .slug
                .as_ref()
                .and_then(|slug| project.tree.plans.get(slug))
            else {
                continue;
            };
            if plan.state == PlanState::Written && listed.insert(plan.slug.clone()) {
                plans.push(choice(plan, true));
            }
        }
    }
    let mut unlisted: Vec<_> = project
        .tree
        .plans
        .values()
        .filter(|plan| plan.state == PlanState::Written && !listed.contains(&plan.slug))
        .collect();
    unlisted.sort_by(|a, b| {
        a.path
            .file_name()
            .cmp(&b.path.file_name())
            .then(a.path.cmp(&b.path))
    });
    plans.extend(unlisted.into_iter().map(|plan| choice(plan, false)));
    plans
}

fn choice(plan: &PlanEntry, listed: bool) -> PlanChoice {
    PlanChoice {
        path: plan.path.to_string_lossy().into_owned(),
        slug: plan.slug.clone(),
        listed,
    }
}

pub(super) fn get_state(req: &ApiRequest<'_>) -> ApiResponse {
    match state_view(req.ctx) {
        Ok(view) => ApiResponse::json(200, &view),
        Err(error) => {
            tracing::warn!("run state could not be read: {error:#}");
            ApiResponse::error(500, "internal", "the run state could not be read")
        }
    }
}

#[cfg(test)]
mod tests;
