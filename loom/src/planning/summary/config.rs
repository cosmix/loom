//! Summary model and effort resolution.

use std::path::Path;

use crate::fs::work_dir::{read_plan_config, PlanConfig, WorkDir};
use crate::user_config::UserConfig;

/// Resolved model and reasoning effort for document summaries and overviews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryModels {
    /// Model used for document summaries.
    pub document_model: String,
    /// Reasoning effort used for document summaries.
    pub document_effort: String,
    /// Model used for overviews and summary sessions.
    pub overview_model: String,
    /// Reasoning effort used for overviews and summary sessions.
    pub overview_effort: String,
}

impl SummaryModels {
    /// Resolve each slot from the project tier, then the user tier and its defaults.
    pub fn resolve(project: &PlanConfig, user: &UserConfig) -> SummaryModels {
        SummaryModels {
            document_model: project
                .document_summary_model()
                .unwrap_or_else(|| user.plan_document_summary_model())
                .to_string(),
            document_effort: project
                .document_summary_effort()
                .unwrap_or_else(|| user.plan_document_summary_effort())
                .to_string(),
            overview_model: project
                .overview_model()
                .unwrap_or_else(|| user.plan_overview_model())
                .to_string(),
            overview_effort: project
                .overview_effort()
                .unwrap_or_else(|| user.plan_overview_effort())
                .to_string(),
        }
    }

    /// Load project and user configuration, using no project overrides without a workspace.
    pub fn load(root: &Path) -> anyhow::Result<SummaryModels> {
        let project = WorkDir::new(root)
            .ok()
            .map(|work_dir| read_plan_config(work_dir.root()))
            .unwrap_or_default();
        Ok(Self::resolve(&project, &UserConfig::load()))
    }
}

#[cfg(test)]
mod tests;
