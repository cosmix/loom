//! Workspace state and the viewer-safe run dialog.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::bail;

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

/// Classify the workspace by its configured plan's filename and daemon liveness. A workspace that
/// names no plan (init was interrupted, or only a config exists) has nothing to run or resume;
/// it is offered as an initialized start that needs `--clean` (`state_view` withholds that
/// when the workspace still holds run state).
pub fn workspace_state(root: &Path) -> anyhow::Result<(WorkspaceState, RunOffer, Option<PathBuf>)> {
    let work_dir = WorkDir::new(root)?;
    if !work_dir.root().exists() || !holds_orchestration_state(work_dir.root()) {
        return Ok((WorkspaceState::Absent, RunOffer::Start, None));
    }
    let Some(source) = get_plan_source_path(&work_dir)? else {
        return Ok((WorkspaceState::Initialized, RunOffer::Start, None));
    };
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
    let preselected = active_plan
        .clone()
        .filter(|_| workspace == WorkspaceState::Initialized)
        .or_else(|| plans.first().map(|plan| plan.path.clone()));
    let (doc_changes, mut blocking_changes) = super::changes::split_changes(root)?;
    // An initialized workspace with no plan is wiped by the start, like a finished one, unless
    // it still holds run state: that would be deleted without abandon's confirmations.
    let nameless = workspace == WorkspaceState::Initialized && active_plan.is_none();
    let kept_state = if nameless {
        orphaned::blocker(root)?
    } else {
        None
    };
    let clean_notice = workspace == WorkspaceState::Done || (nameless && kept_state.is_none());
    blocking_changes.extend(kept_state);
    let abandon = matches!(workspace, WorkspaceState::Running | WorkspaceState::Stopped)
        .then(|| super::abandon::view(ctx))
        .flatten();
    let finish_abandon = (workspace == WorkspaceState::Absent)
        .then(|| super::abandon::stranded_plan(root, ctx.runtime()))
        .flatten();
    Ok(RunStateView {
        clean_notice,
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

/// Features by name, each in metaplan then filename order; standalone plans last.
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
    let mut seen = BTreeSet::new();
    for feature in &project.features {
        for (_, entries) in &feature.metaplans {
            for entry in entries {
                let Some(plan) = entry
                    .slug
                    .as_deref()
                    .and_then(|slug| project.tree.plan(slug))
                else {
                    continue;
                };
                if plan.feature.as_deref() == Some(feature.name.as_str())
                    && plan.state == PlanState::Written
                    && seen.insert(plan.slug.clone())
                {
                    plans.push(choice(plan, true));
                }
            }
        }
        append_unlisted(project, Some(&feature.name), &mut seen, &mut plans);
    }
    append_unlisted(project, None, &mut seen, &mut plans);
    plans
}

fn append_unlisted(
    project: &Project,
    feature: Option<&str>,
    seen: &mut BTreeSet<String>,
    plans: &mut Vec<PlanChoice>,
) {
    let mut unlisted: Vec<_> = project
        .tree
        .plans
        .values()
        .filter(|plan| {
            plan.feature.as_deref() == feature
                && plan.state == PlanState::Written
                && !seen.contains(&plan.slug)
        })
        .collect();
    unlisted.sort_by(|a, b| {
        a.path
            .file_name()
            .cmp(&b.path.file_name())
            .then(a.path.cmp(&b.path))
    });
    for plan in unlisted {
        seen.insert(plan.slug.clone());
        plans.push(choice(plan, false));
    }
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

pub(super) mod orphaned;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod feature_tests {
    use super::*;

    #[test]
    fn features_follow_name_order_with_own_plans_then_standalone_plans() {
        let root = tempfile::tempdir().unwrap();
        for (relative, contents) in [
            (
                "doc/plans/beta/index.md",
                "# Beta\n\n## First `PLAN-b.md`\n\n## Foreign `PLAN-z.md`\n",
            ),
            (
                "doc/plans/alpha/index.md",
                "# Alpha\n\n## Foreign `PLAN-b.md`\n\n## First `PLAN-z.md`\n\n## Standalone `PLAN-00-loose.md`\n",
            ),
            ("doc/plans/alpha/PLAN-z.md", "# Z\n"),
            ("doc/plans/alpha/PLAN-a.md", "# A\n"),
            ("doc/plans/beta/PLAN-b.md", "# B\n"),
            ("doc/plans/beta/PLAN-0-beta.md", "# Unlisted beta\n"),
            ("doc/plans/PLAN-00-loose.md", "# Later standalone\n"),
            ("doc/plans/PLAN-00-loose-extra.md", "# First standalone\n"),
            ("doc/plans/alpha/IN_PROGRESS-PLAN-running.md", "# Running\n"),
            ("doc/plans/alpha/archive/PLAN-archived.md", "# Archived\n"),
        ] {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        let project = Project::load(root.path()).unwrap();

        let plans = project_plans(&project);
        let choices: Vec<_> = plans
            .iter()
            .map(|plan| (plan.path.as_str(), plan.listed))
            .collect();
        assert_eq!(
            choices,
            [
                ("doc/plans/alpha/PLAN-z.md", true),
                ("doc/plans/alpha/PLAN-a.md", false),
                ("doc/plans/beta/PLAN-b.md", true),
                ("doc/plans/beta/PLAN-0-beta.md", false),
                ("doc/plans/PLAN-00-loose-extra.md", false),
                ("doc/plans/PLAN-00-loose.md", false),
            ]
        );
    }
}
