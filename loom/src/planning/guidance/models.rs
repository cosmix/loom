//! Model and effort resolution for planning guidance sessions and agents.

use std::path::Path;

use crate::fs::work_dir::{read_plan_config, PlanConfig, WorkDir};
use crate::planning::summary::config::SummaryModels;
use crate::user_config::UserConfig;

/// A resolved model and reasoning effort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEffort {
    pub model: String,
    pub effort: String,
}

/// Resolved settings for authoring sessions and summary agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidanceModels {
    pub spec: ModelEffort,
    pub plan: ModelEffort,
    pub summary_session: ModelEffort,
    pub document_agent: ModelEffort,
    pub overview_agent: ModelEffort,
}

impl GuidanceModels {
    /// Resolve each setting from the project tier, then the user tier and its defaults.
    pub fn resolve(project: &PlanConfig, user: &UserConfig) -> GuidanceModels {
        let summary = SummaryModels::resolve(project, user);
        let overview = ModelEffort {
            model: summary.overview_model,
            effort: summary.overview_effort,
        };
        GuidanceModels {
            spec: ModelEffort {
                model: project
                    .spec_authoring_model()
                    .unwrap_or_else(|| user.plan_spec_authoring_model())
                    .to_string(),
                effort: project
                    .spec_authoring_effort()
                    .unwrap_or_else(|| user.plan_spec_authoring_effort())
                    .to_string(),
            },
            plan: ModelEffort {
                model: project
                    .plan_authoring_model()
                    .unwrap_or_else(|| user.plan_plan_authoring_model())
                    .to_string(),
                effort: project
                    .plan_authoring_effort()
                    .unwrap_or_else(|| user.plan_plan_authoring_effort())
                    .to_string(),
            },
            summary_session: overview.clone(),
            document_agent: ModelEffort {
                model: summary.document_model,
                effort: summary.document_effort,
            },
            overview_agent: overview,
        }
    }

    /// Load project and user settings, using no project overrides without a workspace.
    pub fn load(root: &Path) -> anyhow::Result<GuidanceModels> {
        let project = WorkDir::new(root)
            .ok()
            .map(|work_dir| read_plan_config(work_dir.root()))
            .unwrap_or_default();
        Ok(Self::resolve(&project, &UserConfig::load()))
    }
}

#[cfg(test)]
mod tests;
